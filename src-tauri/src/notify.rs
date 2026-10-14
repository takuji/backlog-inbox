//! OS通知(トースト)の表示と、macOS でのクリック→ページ遷移。

use crate::backlog::NotificationDto;
use crate::i18n;
use crate::state::AppState;
use tauri::{AppHandle, Manager};
#[cfg(not(target_os = "macos"))]
use tauri_plugin_notification::NotificationExt;

fn toast(app: &AppHandle, title: &str, body: &str, url: Option<&str>) {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        toast_macos(title, body, url);
    }
    #[cfg(not(target_os = "macos"))]
    {
        // 非macOS(Windows/Linux)はクリック非対応。通知表示のみ。
        let _ = url;
        let _ = app.notification().builder().title(title).body(body).show();
    }
}

// クリック待ちの通知は send_notification がブロックするため、通知ごとに専用
// スレッドで待つ。ユーザーが放置した通知はスレッドが残るので、同時待ち数に
// 上限を設け、超過分はクリック非対応(表示のみ)にしてスレッド増加を防ぐ。
#[cfg(target_os = "macos")]
static PENDING_CLICKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(target_os = "macos")]
const MAX_PENDING_CLICKS: usize = 32;

#[cfg(target_os = "macos")]
fn toast_macos(title: &str, body: &str, url: Option<&str>) {
    use std::sync::atomic::Ordering::Relaxed;

    let wait_for_click = url.is_some() && PENDING_CLICKS.load(Relaxed) < MAX_PENDING_CLICKS;
    let title = title.to_string();
    let body = body.to_string();
    let url = url.map(str::to_string);
    if wait_for_click {
        PENDING_CLICKS.fetch_add(1, Relaxed);
    }
    // 待ち続けるスレッドが多数残ってもメモリを圧迫しないよう小さめのスタックで起動
    let spawned = std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || {
            let mut opts = mac_notification_sys::Notification::new();
            if wait_for_click {
                opts.wait_for_click(true);
            }
            let resp = mac_notification_sys::send_notification(&title, None, &body, Some(&opts));
            if wait_for_click {
                PENDING_CLICKS.fetch_sub(1, Relaxed);
            }
            if let (Ok(mac_notification_sys::NotificationResponse::Click), Some(url)) = (resp, url)
            {
                let _ = tauri_plugin_opener::open_url(url, None::<&str>);
            }
        });
    if spawned.is_err() && wait_for_click {
        PENDING_CLICKS.fetch_sub(1, Relaxed);
    }
}

/// 取得した通知の中から新着を判定し、トーストで知らせる。
pub(crate) fn notify_new_arrivals(
    app: &AppHandle,
    notifications: &[NotificationDto],
    unread_count: i64,
) {
    let state = app.state::<AppState>();
    let lang = state.lang();
    let (prev, is_baseline) = {
        let mut last = state.last_notified_id.lock().unwrap();
        let prev = *last;
        let max_id = notifications.iter().map(|n| n.id).max().unwrap_or(prev);
        if max_id > prev {
            *last = max_id;
        }
        (prev, prev == 0)
    };
    if is_baseline {
        // 起動直後の初回取得。個別のトーストは出さず未読の存在だけ知らせる
        if unread_count > 0 {
            toast(
                app,
                "Backlog",
                &i18n::toast_unread_summary(unread_count, lang),
                None,
            );
        }
        return;
    }
    let fresh: Vec<&NotificationDto> = notifications
        .iter()
        .filter(|n| n.id > prev && !n.already_read)
        .collect();
    for n in fresh.iter().take(3) {
        toast(
            app,
            &n.title,
            &i18n::toast_new_body(&n.sender_name, n.reason, lang),
            Some(&n.url),
        );
    }
    if fresh.len() > 3 {
        toast(
            app,
            "Backlog",
            &i18n::toast_more(fresh.len() - 3, lang),
            None,
        );
    }
}
