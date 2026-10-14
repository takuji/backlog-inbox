//! 接続設定(settings.json + OSセキュアストア)と環境設定(prefs.json)の永続化。

use crate::backlog::Settings;
use crate::i18n::{self, Lang};
use crate::state::AppState;
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "org.smkw.backlog-inbox";
const KEYRING_USER: &str = "backlog-api-key";

/// settings.json の保存形式。APIキーはOSのセキュアストアに保存するため
/// ここには含めない(api_keyは平文保存していた旧形式の読み込み専用)
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSettings {
    space_host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

fn keyring_entry(lang: Lang) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .map_err(|e| i18n::err_secure_store(&e.to_string(), lang))
}

pub(crate) fn store_api_key(api_key: &str, lang: Lang) -> Result<(), String> {
    keyring_entry(lang)?
        .set_password(api_key)
        .map_err(|e| i18n::err_save_key(&e.to_string(), lang))
}

fn load_api_key() -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
        .ok()?
        .get_password()
        .ok()
}

/// キーチェーンから API キーを削除する。
///
/// **エントリが無い場合(`NoEntry`)は成功扱い。** 消したいものが既に無いだけ
/// なので冪等にする。ここだけは実OSのセキュアストアを触るためユニット
/// テストできないので、分岐を1つに抑えている。
fn delete_api_key(lang: Lang) -> Result<(), String> {
    match keyring_entry(lang)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(i18n::err_clear_key(&e.to_string(), lang)),
    }
}

/// settings.json を削除する。**存在しなければ成功扱い。**
pub(crate) fn remove_settings_file(path: &std::path::Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// 接続情報(キーチェーンの API キーと settings.json)を消す。
///
/// **順序が重要。鍵を先、ファイルを後。** `load_settings` は API キーが取れ
/// なければ `None` を返す(下の `load_settings` 参照)ので、鍵を先に消せば
/// 後段で失敗しても未接続は確定する。逆順にすると「解除できたように見えて
/// 鍵がキーチェーンに残る」最悪の状態を作りうる。
///
/// 鍵の削除に失敗したときは何も変えずに `Err` を返す(呼び出し側は
/// AppState を触らない)。
///
/// ファイルの削除に失敗したときは、空の spaceHost で上書きして無効化する。
/// 旧形式(API キーを平文保存)のファイルをそのまま放置すると、次の起動時に
/// `load_settings` が平文キーを読んでキーチェーンへ書き戻し、接続が復活して
/// しまうため。
///
/// **上書きにも失敗したら `Err` を返す。** そこまで来ると鍵は既に消えている
/// のに旧形式の平文キーだけが残るので、成功と伝えるわけにいかない。
/// 「解除できたはずなのに再起動したら接続されている」が、キーチェーンを
/// 先に消す順序で防ごうとしている当のものなので、利用者に知らせる。
pub(crate) fn clear_connection(app: &AppHandle, lang: Lang) -> Result<(), String> {
    delete_api_key(lang)?;
    if let Ok(path) = settings_path(app) {
        if let Err(remove_err) = remove_settings_file(&path) {
            // 上書きが失敗する理由は削除が失敗した理由とほぼ同じ(権限や
            // I/O)なので、原因として意味のある削除側のエラーを伝える。
            write_settings_file(&path, "", lang)
                .map_err(|_| i18n::err_clear_settings_file(&remove_err, lang))?;
        }
    }
    Ok(())
}

pub(crate) fn settings_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("settings.json"))
        .map_err(|e| e.to_string())
}

pub(crate) fn write_settings_file(
    path: &std::path::Path,
    space_host: &str,
    lang: Lang,
) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| i18n::err_save_settings(&e.to_string(), lang))?;
    }
    let stored = StoredSettings {
        space_host: space_host.to_string(),
        api_key: None,
    };
    let json = serde_json::to_vec_pretty(&stored).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| i18n::err_save_settings(&e.to_string(), lang))
}

pub(crate) fn load_settings(app: &AppHandle, lang: Lang) -> Option<Settings> {
    let path = settings_path(app).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    let stored: StoredSettings = serde_json::from_str(&raw).ok()?;

    // 旧形式(APIキーを平文保存)からの移行: セキュアストアへ移し、ファイルから消す
    if let Some(api_key) = stored.api_key.filter(|k| !k.is_empty()) {
        if store_api_key(&api_key, lang).is_ok() {
            let _ = write_settings_file(&path, &stored.space_host, lang);
        }
        return Some(Settings {
            space_host: stored.space_host,
            api_key,
        });
    }

    let api_key = load_api_key()?;
    Some(Settings {
        space_host: stored.space_host,
        api_key,
    })
}

// --- 環境設定の永続化(prefs.json、接続前でも保存できるよう settings とは分離) ---

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct Prefs {
    pub(crate) language: Option<String>,
    /// メニューバーに未読件数を数字で表示するか(既定は false)
    pub(crate) show_unread_count: bool,
    /// MCPサーバーを有効にするか(既定は false)
    pub(crate) mcp_enabled: bool,
}

fn prefs_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|d| d.join("prefs.json"))
        .map_err(|e| e.to_string())
}

pub(crate) fn load_prefs(app: &AppHandle) -> Prefs {
    prefs_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_prefs(app: &AppHandle, prefs: &Prefs) {
    if let Ok(path) = prefs_path(app) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(prefs) {
            let _ = std::fs::write(path, json);
        }
    }
}

/// 現在のメモリ上の設定値を prefs.json に書き出す。
pub(crate) fn persist_prefs(app: &AppHandle) {
    let state = app.state::<AppState>();
    let prefs = Prefs {
        language: Some(state.lang().code().to_string()),
        show_unread_count: *state.show_unread_count.lock().unwrap(),
        mcp_enabled: *state.mcp_enabled.lock().unwrap(),
    };
    save_prefs(app, &prefs);
}

pub(crate) fn normalize_host(input: &str, lang: Lang) -> Result<String, String> {
    let mut host = input.trim().to_string();
    for prefix in ["https://", "http://"] {
        if let Some(rest) = host.strip_prefix(prefix) {
            host = rest.to_string();
        }
    }
    if let Some(i) = host.find('/') {
        host.truncate(i);
    }
    if host.is_empty() || !host.contains('.') {
        return Err(i18n::err_space_url(lang));
    }
    Ok(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_mcp_enabled_defaults_to_false_when_missing() {
        // 既存ユーザーの prefs.json には mcpEnabled が無い
        let prefs: Prefs =
            serde_json::from_str(r#"{"language":"ja","showUnreadCount":true}"#).unwrap();
        assert!(!prefs.mcp_enabled);
        assert!(prefs.show_unread_count);
    }

    #[test]
    fn prefs_mcp_enabled_roundtrip() {
        let prefs: Prefs = serde_json::from_str(r#"{"mcpEnabled":true}"#).unwrap();
        assert!(prefs.mcp_enabled);
        let json = serde_json::to_string(&prefs).unwrap();
        assert!(json.contains("\"mcpEnabled\":true"), "got: {json}");
    }

    /// テスト用の一時パス。並列実行される他テストとぶつからないよう、
    /// プロセスIDとテスト名で固有名にする(`tempfile` は追加しない)。
    fn temp_settings_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("backlog-inbox-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn remove_settings_file_deletes_an_existing_file() {
        let path = temp_settings_path("remove-existing");
        std::fs::write(&path, b"{\"spaceHost\":\"example.backlog.com\"}").unwrap();
        assert!(path.exists());

        remove_settings_file(&path).unwrap();

        assert!(!path.exists());
    }

    /// **存在しなければ成功扱いにすること(冪等性)。**
    ///
    /// 消したいものが既に無いだけなので、エラーにすると「解除できない」
    /// 状態から抜け出せなくなる。
    #[test]
    fn remove_settings_file_succeeds_when_the_file_is_missing() {
        let path = temp_settings_path("remove-missing");
        let _ = std::fs::remove_file(&path);
        assert!(!path.exists());

        assert!(remove_settings_file(&path).is_ok());
    }
}
