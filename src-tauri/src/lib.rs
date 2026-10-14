mod backlog;
mod commands;
mod i18n;
mod mcp;
mod notify;
mod refresh;
mod settings;
mod state;
mod tray;

use backlog::Client;
use i18n::Lang;
use refresh::{fetch_issues, fetch_notifications, fetch_watchings, refresh_all, spawn_poller};
use settings::{load_prefs, load_settings};
use state::{AppState, NotifStore, TrayIcons};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::image::Image;
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::MacosLauncher;
#[cfg(not(target_os = "macos"))]
use tray::make_unread_icon;
use tray::{build_tray_menu, TRAY_ID};

// 各データの自動更新間隔(それぞれ独立)。通知は要件どおり3分。
const NOTIF_INTERVAL_SECS: u64 = 180;
const ISSUES_INTERVAL_SECS: u64 = 300;
const WATCHINGS_INTERVAL_SECS: u64 = 600;

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        // ユーザーが変更したウィンドウサイズを保存・復元する(位置は対象外)
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(tauri_plugin_window_state::StateFlags::SIZE)
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();

            // macOS: 通知にアプリのバンドルIDを紐づける(名前・アイコン表示のため)。
            // 署名済みバンドルで有効。dev(未バンドル)では失敗しうるので無視する。
            #[cfg(target_os = "macos")]
            {
                let _ = mac_notification_sys::set_application(&app.config().identifier);
            }

            let prefs = load_prefs(&handle);

            // 表示言語: 永続化された設定があればそれを、なければOSロケールから既定を決める
            let startup_lang = prefs
                .language
                .as_deref()
                .and_then(Lang::from_code)
                .unwrap_or_else(Lang::detect_default);
            let lang = Arc::new(Mutex::new(startup_lang));

            app.manage(AppState {
                client: Client::new(lang.clone()),
                settings: Mutex::new(load_settings(&handle, startup_lang)),
                me: Mutex::new(None),
                space: Mutex::new(None),
                last_notified_id: Mutex::new(0),
                notifications: Mutex::new(NotifStore::default()),
                snapshot: Mutex::new(None),
                lang,
                show_unread_count: Mutex::new(prefs.show_unread_count),
                icon_cache: Mutex::new(HashMap::new()),
                space_logo: Mutex::new(None),
                last_watching_count: Mutex::new(None),
                last_issues_count: Mutex::new(None),
                mcp_enabled: Mutex::new(prefs.mcp_enabled),
                mcp_server: Mutex::new(None),
                mcp_error: Mutex::new(None),
                session_generation: std::sync::atomic::AtomicU64::new(0),
            });

            // macOSのメニューバーは明暗に追従する単色テンプレートアイコンが作法なので
            // 専用画像を使う。その他のプラットフォームはカラー版に赤ドットを実行時合成する
            #[cfg(target_os = "macos")]
            let (normal, unread) = (
                Image::from_bytes(include_bytes!("../icons/tray-mac.png"))?,
                Image::from_bytes(include_bytes!("../icons/tray-mac-unread.png"))?,
            );
            #[cfg(not(target_os = "macos"))]
            let (normal, unread) = {
                let base = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
                let unread = make_unread_icon(&base);
                (base, unread)
            };
            app.manage(TrayIcons {
                normal: normal.clone(),
                unread,
            });

            let menu = build_tray_menu(app.handle(), startup_lang)?;

            let tray_builder = TrayIconBuilder::with_id(TRAY_ID)
                .icon(normal)
                .tooltip("Backlog Inbox")
                .menu(&menu)
                .show_menu_on_left_click(true)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "refresh" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            // トレイの「今すぐ更新」は手動更新なので通知一覧も必ず取得する
                            if let Err(e) = refresh_all(&app, true, true).await {
                                let _ = app.emit("refresh-error", e);
                            }
                        });
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick { .. } = event {
                        show_main_window(tray.app_handle());
                    }
                });
            // テンプレート指定でシステムがメニューバーの明暗に合わせて描画する
            #[cfg(target_os = "macos")]
            let tray_builder = tray_builder.icon_as_template(true);
            tray_builder.build(app)?;

            // 自動起動時(--hidden)はウィンドウを出さずに常駐だけする
            let start_hidden = std::env::args().any(|a| a == "--hidden");
            if !start_hidden {
                show_main_window(&handle);
            }

            // dev実行中のバイナリをログイン項目に登録しないようreleaseビルドに限定
            #[cfg(not(debug_assertions))]
            {
                use tauri_plugin_autostart::ManagerExt;
                let _ = app.autolaunch().enable();
            }

            // 通知・課題・ウォッチを、それぞれ独立した間隔でポーリングする。
            spawn_poller(handle.clone(), NOTIF_INTERVAL_SECS, |app| async move {
                // 通知は count ゲート(一覧は変化時のみ取得)、新着はトースト
                fetch_notifications(&app, true, false).await
            });
            spawn_poller(handle.clone(), ISSUES_INTERVAL_SECS, |app| async move {
                fetch_issues(&app, false).await
            });
            spawn_poller(handle.clone(), WATCHINGS_INTERVAL_SECS, |app| async move {
                fetch_watchings(&app, false).await
            });

            // MCPサーバー: 有効化されていて接続設定があれば自動起動する
            if prefs.mcp_enabled {
                let handle2 = handle.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = mcp::start(&handle2).await;
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // 閉じる操作では終了せず非表示にして常駐を続ける
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::save_settings,
            commands::clear_settings,
            commands::get_snapshot,
            commands::refresh,
            commands::refresh_issues,
            commands::load_older_notifications,
            commands::mark_notification_read,
            commands::mark_resource_viewed,
            commands::mark_all_read,
            commands::open_url,
            commands::get_language,
            commands::set_language,
            commands::get_show_unread_count,
            commands::set_show_unread_count,
            commands::get_user_icon,
            commands::get_space_logo,
            commands::get_mcp_status,
            commands::set_mcp_enabled
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app, _event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = _event {
                show_main_window(_app);
            }
        });
}
