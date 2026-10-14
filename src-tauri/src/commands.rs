//! フロントエンドから呼ばれる `#[tauri::command]` 群。

use crate::backlog::{Settings, Snapshot, User};
use crate::i18n::{self, Lang};
use crate::refresh::{fetch_issues, load_older, refresh_all};
use crate::settings::{
    clear_connection, normalize_host, persist_prefs, settings_path, store_api_key,
    write_settings_file,
};
use crate::state::AppState;
use crate::tray::{build_tray_menu, current_unread, update_tray, TRAY_ID};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
pub(crate) fn get_settings(state: State<'_, AppState>) -> Option<Settings> {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
pub(crate) async fn save_settings(
    app: AppHandle,
    space_host: String,
    api_key: String,
) -> Result<User, String> {
    let lang = app.state::<AppState>().lang();
    let host = normalize_host(&space_host, lang)?;
    let api_key = api_key.trim().to_string();
    if api_key.is_empty() {
        return Err(i18n::err_api_key_required(lang));
    }
    let settings = Settings {
        space_host: host,
        api_key,
    };

    let client = app.state::<AppState>().client.clone();
    let user = client.myself(&settings).await?;

    // APIキーはOSのセキュアストアへ、それ以外の設定はファイルへ保存する
    store_api_key(&settings.api_key, lang)?;
    let path = settings_path(&app)?;
    write_settings_file(&path, &settings.space_host, lang)?;

    {
        let state = app.state::<AppState>();
        state.reset_session();
        *state.settings.lock().unwrap() = Some(settings);
        *state.me.lock().unwrap() = Some(user.clone());
    }
    let _ = refresh_all(&app, false, true).await;
    // MCPサーバーの実体を mcp_enabled に揃える。解除でハンドルが落ちた
    // 後の再接続でも、ここで prefs どおりに起動し直す。
    crate::mcp::sync_with_prefs(&app).await;
    Ok(user)
}

/// 接続情報を消して未接続状態に戻す。`save_settings` の鏡像。
///
/// `prefs.json`(言語・未読件数表示・MCP有効フラグ)は消さない。接続を
/// 切ることは表示設定の変更意図ではないため。
#[tauri::command]
pub(crate) async fn clear_settings(app: AppHandle) -> Result<(), String> {
    let lang = app.state::<AppState>().lang();
    // 鍵の削除に失敗したらここで返る。AppState は手つかずなので、
    // ユーザーはエラーを見たあとも接続済みのまま使える。
    // ファイルの削除・上書きに失敗した場合も Err で返るが、その時は
    // 鍵は既に削除されているので、UI に見える接続状態は stale。
    // ユーザーはエラーを見て、解除が完全でなかったことを認識する。
    clear_connection(&app, lang)?;
    app.state::<AppState>().reset_session();
    // 資格情報が無くなったのでMCPサーバーは動かせない。止めたなら報告
    // すべき起動エラーも無いので消す(`set_mcp_enabled(false)` と同じ扱い)。
    // 有効フラグ `mcp_enabled` は**ユーザーの意図**なので据え置き、
    // 次に接続したら `save_settings` の `mcp::sync_with_prefs` が
    // prefs に従って起動し直す。
    crate::mcp::stop(&app).await;
    *app.state::<AppState>().mcp_error.lock().unwrap() = None;
    // 未読表示を消す。reset_session 後は snapshot が None なので
    // `current_unread` も 0 を返すが、意図を明示するため 0 を直接渡す。
    // 非同期コマンドから `update_tray` を直接呼ぶのは
    // `mark_notification_read`(commands.rs:113)と同じ形。
    update_tray(&app, 0);
    // **`snapshot` イベントは emit しない。** 未接続を表す Snapshot という
    // 概念が無く、`Snapshot::default()` を流すと「スペース名も件数も空の
    // 接続済み画面」を描かせてしまう。フロントは自分で state を落とす。
    Ok(())
}

#[tauri::command]
pub(crate) fn get_snapshot(state: State<'_, AppState>) -> Option<Snapshot> {
    state.snapshot.lock().unwrap().clone()
}

#[tauri::command]
pub(crate) async fn refresh(app: AppHandle) -> Result<Snapshot, String> {
    refresh_all(&app, false, true).await
}

/// 担当課題タブを開いたときに呼ぶ。件数ゲートを介さず必ず一覧を取得する。
#[tauri::command]
pub(crate) async fn refresh_issues(app: AppHandle) -> Result<Snapshot, String> {
    fetch_issues(&app, true).await
}

#[tauri::command]
pub(crate) async fn mark_notification_read(app: AppHandle, id: i64) -> Result<(), String> {
    let (client, settings) = {
        let state = app.state::<AppState>();
        let cloned = (state.client.clone(), state.settings.lock().unwrap().clone());
        cloned
    };
    let settings =
        settings.ok_or_else(|| i18n::err_not_configured(app.state::<AppState>().lang()))?;
    client.mark_read(&settings, id).await?;

    // 次回ポーリングを待たずにストア・キャッシュ・トレイ表示を更新する
    let state = app.state::<AppState>();
    let became_read = {
        let mut store = state.notifications.lock().unwrap();
        match store.items.iter_mut().find(|n| n.id == id) {
            Some(n) if !n.already_read => {
                n.already_read = true;
                true
            }
            _ => false,
        }
    };
    let updated = {
        let store = state.notifications.lock().unwrap();
        let mut guard = state.snapshot.lock().unwrap();
        guard.as_mut().map(|snap| {
            if became_read {
                snap.unread_count = (snap.unread_count - 1).max(0);
            }
            snap.notifications = store.items.clone();
            snap.clone()
        })
    };
    if let Some(snap) = updated {
        update_tray(&app, snap.unread_count);
        let _ = app.emit("snapshot", &snap);
    }
    Ok(())
}

/// クリックで開いた通知を「リソース閲覧済み」としてストアに記録する。
///
/// **snapshot は emit しない。** ここで emit すると、いま見ている一覧が即座に
/// 組み直され、未閲覧で絞り込んでいるとカーソルの下から項目が消える。続けて
/// クリックすると意図しないページを開く事故になるので、反映は次の更新・
/// ポーリングで届く snapshot に任せる。画面上の即時の手応えは、フロント側の
/// `viewedIds` による暗転が担っている。
#[tauri::command]
pub(crate) fn mark_resource_viewed(app: AppHandle, id: i64) {
    app.state::<AppState>()
        .notifications
        .lock()
        .unwrap()
        .mark_resource_viewed(id);
}

#[tauri::command]
pub(crate) async fn mark_all_read(app: AppHandle) -> Result<Snapshot, String> {
    let (client, settings) = {
        let state = app.state::<AppState>();
        let cloned = (state.client.clone(), state.settings.lock().unwrap().clone());
        cloned
    };
    let settings =
        settings.ok_or_else(|| i18n::err_not_configured(app.state::<AppState>().lang()))?;
    client.mark_all_read(&settings).await?;
    // 取得済みの古い通知も含めてローカルの既読状態を揃える
    {
        let state = app.state::<AppState>();
        let mut store = state.notifications.lock().unwrap();
        for n in store.items.iter_mut() {
            n.already_read = true;
        }
    }
    refresh_all(&app, false, true).await
}

#[tauri::command]
pub(crate) async fn load_older_notifications(app: AppHandle) -> Result<Snapshot, String> {
    load_older(&app).await
}

#[tauri::command]
pub(crate) fn open_url(url: String) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// ユーザーアイコンを data URI で返す。取得済みならキャッシュから返す。
#[tauri::command]
pub(crate) async fn get_user_icon(app: AppHandle, user_id: i64) -> Result<String, String> {
    if let Some(uri) = app
        .state::<AppState>()
        .icon_cache
        .lock()
        .unwrap()
        .get(&user_id)
        .cloned()
    {
        return Ok(uri);
    }
    let (client, settings) = {
        let state = app.state::<AppState>();
        let cloned = (state.client.clone(), state.settings.lock().unwrap().clone());
        cloned
    };
    let settings =
        settings.ok_or_else(|| i18n::err_not_configured(app.state::<AppState>().lang()))?;
    let (bytes, mime) = client.user_icon(&settings, user_id).await?;
    let uri = format!(
        "data:{};base64,{}",
        mime,
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
    );
    app.state::<AppState>()
        .icon_cache
        .lock()
        .unwrap()
        .insert(user_id, uri.clone());
    Ok(uri)
}

/// スペースロゴを data URI で返す。取得済みならキャッシュから返す。
#[tauri::command]
pub(crate) async fn get_space_logo(app: AppHandle) -> Result<String, String> {
    if let Some(uri) = app.state::<AppState>().space_logo.lock().unwrap().clone() {
        return Ok(uri);
    }
    let (client, settings) = {
        let state = app.state::<AppState>();
        let cloned = (state.client.clone(), state.settings.lock().unwrap().clone());
        cloned
    };
    let settings =
        settings.ok_or_else(|| i18n::err_not_configured(app.state::<AppState>().lang()))?;
    let (bytes, mime) = client.space_logo(&settings).await?;
    let uri = format!(
        "data:{};base64,{}",
        mime,
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
    );
    *app.state::<AppState>().space_logo.lock().unwrap() = Some(uri.clone());
    Ok(uri)
}

#[tauri::command]
pub(crate) fn get_language(state: State<'_, AppState>) -> String {
    state.lang().code().to_string()
}

#[tauri::command]
pub(crate) fn set_language(app: AppHandle, lang: String) -> Result<(), String> {
    let new_lang = Lang::from_code(&lang).ok_or_else(|| format!("unknown language: {}", lang))?;
    *app.state::<AppState>().lang.lock().unwrap() = new_lang;
    persist_prefs(&app);
    // トレイのメニューとツールチップを新しい言語で更新する(メニュー操作は
    // メインスレッドで行う必要があるため run_on_main_thread に載せる)
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(tray) = app2.tray_by_id(TRAY_ID) {
            if let Ok(menu) = build_tray_menu(&app2, new_lang) {
                let _ = tray.set_menu(Some(menu));
            }
        }
        update_tray(&app2, current_unread(&app2));
    });
    Ok(())
}

#[tauri::command]
pub(crate) fn get_show_unread_count(state: State<'_, AppState>) -> bool {
    *state.show_unread_count.lock().unwrap()
}

#[tauri::command]
pub(crate) fn set_show_unread_count(app: AppHandle, value: bool) -> Result<(), String> {
    *app.state::<AppState>().show_unread_count.lock().unwrap() = value;
    persist_prefs(&app);
    // トレイ表示は必ずメインスレッドで更新する
    let app2 = app.clone();
    let _ = app.run_on_main_thread(move || {
        update_tray(&app2, current_unread(&app2));
    });
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpStatus {
    enabled: bool,
    running: bool,
    url: Option<String>,
    error: Option<String>,
}

fn mcp_status(app: &AppHandle) -> McpStatus {
    let state = app.state::<AppState>();
    let running = state.mcp_server.lock().unwrap().is_some();
    let status = McpStatus {
        enabled: *state.mcp_enabled.lock().unwrap(),
        running,
        url: running.then(crate::mcp::mcp_url),
        error: state.mcp_error.lock().unwrap().clone(),
    };
    status
}

#[tauri::command]
pub(crate) fn get_mcp_status(app: AppHandle) -> McpStatus {
    mcp_status(&app)
}

#[tauri::command]
pub(crate) async fn set_mcp_enabled(app: AppHandle, enabled: bool) -> Result<McpStatus, String> {
    // ユーザーの意図は常に保存する(起動失敗してもONのまま=次回アプリ起動時に再試行)
    *app.state::<AppState>().mcp_enabled.lock().unwrap() = enabled;
    persist_prefs(&app);
    if enabled {
        // 失敗は McpStatus.error として返す(Errにしない)
        let _ = crate::mcp::start(&app).await;
    } else {
        crate::mcp::stop(&app).await;
        *app.state::<AppState>().mcp_error.lock().unwrap() = None;
    }
    Ok(mcp_status(&app))
}
