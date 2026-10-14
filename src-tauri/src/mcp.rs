//! 組み込みMCPサーバー(backlog-mcp)のライフサイクル管理。
//!
//! prefs の mcp_enabled が「ユーザーの意図」、AppState.mcp_server の有無が「実際の状態」。
//! 起動失敗は AppState.mcp_error に保持して設定画面に表示する。

use crate::backlog::base_url;
use crate::i18n;
use crate::state::AppState;
use backlog_mcp::BacklogMcpConfig;
use tauri::{AppHandle, Manager};

pub(crate) const MCP_PORT: u16 = 39150;

pub(crate) fn mcp_url() -> String {
    format!("http://127.0.0.1:{MCP_PORT}/mcp")
}

/// AppStateの接続設定でMCPサーバーを起動する。すでに稼働中なら何もしない。
pub(crate) async fn start(app: &AppHandle) -> Result<(), String> {
    let config = {
        let state = app.state::<AppState>();
        if state.mcp_server.lock().unwrap().is_some() {
            return Ok(());
        }
        let Some(settings) = state.settings.lock().unwrap().clone() else {
            let msg = i18n::err_not_configured(state.lang());
            *state.mcp_error.lock().unwrap() = Some(msg.clone());
            return Err(msg);
        };
        BacklogMcpConfig {
            base_url: base_url(&settings.space_host),
            api_key: settings.api_key,
        }
    };
    let result = backlog_mcp::serve_http(config, MCP_PORT).await;
    let state = app.state::<AppState>();
    match result {
        Ok(handle) => {
            *state.mcp_server.lock().unwrap() = Some(handle);
            *state.mcp_error.lock().unwrap() = None;
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string();
            *state.mcp_error.lock().unwrap() = Some(msg.clone());
            Err(msg)
        }
    }
}

/// 稼働中なら停止する。
pub(crate) async fn stop(app: &AppHandle) {
    let handle = app.state::<AppState>().mcp_server.lock().unwrap().take();
    if let Some(handle) = handle {
        // プロセス内サーバーの停止失敗は実害がないため無視する
        let _ = handle.shutdown().await;
    }
}

/// 接続設定を保存したあと、サーバーの実体を `mcp_enabled`(ユーザーの意図)へ
/// 揃える。稼働中なら一度止め、有効なら新しい資格情報で起動し直す。
///
/// **「稼働中なら再起動」では足りない。** 接続解除(`clear_settings`)が
/// ハンドルを落とすので、同じセッションで接続し直しても二度と起動せず、
/// チェックボックスは ON のまま実体だけ動かない状態になる。起動時に
/// 資格情報が無くて起動できなかった場合も同じで、最初の接続成功で拾える。
///
/// 起動失敗は `mcp_error` に載せるだけで保存自体は失敗させない
/// (`set_mcp_enabled` と同じ方針)。
pub(crate) async fn sync_with_prefs(app: &AppHandle) {
    // 稼働中でなければ `stop` は何もしない
    stop(app).await;
    let enabled = *app.state::<AppState>().mcp_enabled.lock().unwrap();
    if enabled {
        let _ = start(app).await;
    }
}
