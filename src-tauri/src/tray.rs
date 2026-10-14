//! トレイ(メニューバー)アイコン・ツールチップ・メニューの構築と更新。

use crate::i18n::{self, Lang};
use crate::state::{AppState, TrayIcons};
#[cfg(not(target_os = "macos"))]
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::{AppHandle, Manager};

pub(crate) const TRAY_ID: &str = "main-tray";

/// 非macOS向け: カラーアイコンの右上に赤ドットを合成して未読アイコンを作る。
#[cfg(not(target_os = "macos"))]
pub(crate) fn make_unread_icon(base: &Image<'_>) -> Image<'static> {
    let w = base.width();
    let h = base.height();
    let mut rgba = base.rgba().to_vec();
    let r = (w.min(h) as f32) * 0.26;
    let cx = w as f32 - r - 1.0;
    let cy = r + 1.0;
    for y in 0..h {
        for x in 0..w {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if dx * dx + dy * dy <= r * r {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = 0xE5;
                rgba[i + 1] = 0x48;
                rgba[i + 2] = 0x48;
                rgba[i + 3] = 0xFF;
            }
        }
    }
    Image::new_owned(rgba, w, h)
}

/// トレイのアイコン・ツールチップ(・macOSの未読数タイトル/Dockバッジ)を更新する。
pub(crate) fn update_tray(app: &AppHandle, unread: i64) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let icons = app.state::<TrayIcons>();
        let icon = if unread > 0 {
            icons.unread.clone()
        } else {
            icons.normal.clone()
        };
        // set_icon はmacOSのテンプレートフラグをfalseで上書きするため、
        // フラグを同時指定できるAPIで更新する(macOS以外では set_icon 相当)
        let _ = tray.set_icon_with_as_template(Some(icon), cfg!(target_os = "macos"));
        let tooltip = i18n::tray_tooltip(unread, app.state::<AppState>().lang());
        let _ = tray.set_tooltip(Some(tooltip.as_str()));
        #[cfg(target_os = "macos")]
        {
            // 設定で有効なときだけ未読件数を数字表示する。100件以上は「99+」で頭打ち。
            // 無効時・0件時は空文字を渡してタイトルを消す(set_title(None) は
            // ライブラリの都合で既存表示をクリアできないため)。
            let show = *app.state::<AppState>().show_unread_count.lock().unwrap();
            let title = if !show || unread <= 0 {
                String::new()
            } else if unread > 99 {
                "99+".to_string()
            } else {
                unread.to_string()
            };
            let _ = tray.set_title(Some(&title));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_badge_count(if unread > 0 { Some(unread) } else { None });
    }
}

/// トレイの右クリックメニューを指定言語で構築する。
pub(crate) fn build_tray_menu(app: &AppHandle, lang: Lang) -> tauri::Result<Menu<tauri::Wry>> {
    let open = MenuItem::with_id(app, "open", i18n::menu_open(lang), true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", i18n::menu_refresh(lang), true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", i18n::menu_quit(lang), true, None::<&str>)?;
    Menu::with_items(app, &[&open, &refresh, &quit])
}

/// 直近のスナップショットの未読数(なければ 0)。
pub(crate) fn current_unread(app: &AppHandle) -> i64 {
    app.state::<AppState>()
        .snapshot
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.unread_count)
        .unwrap_or(0)
}
