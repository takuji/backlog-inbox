//! データ取得エンジン。通知・課題・ウォッチをそれぞれ独立して更新し、
//! Snapshot を組み立てて emit する。定期ポーリングもここで回す。

use crate::backlog::{Client, NotificationDto, Settings, Snapshot, Space, User};
use crate::i18n;
use crate::notify::notify_new_arrivals;
use crate::state::AppState;
use crate::tray::update_tray;
use tauri::{AppHandle, Emitter, Manager};

/// 通知一覧・続きの有無・スキップ件数をまとめて `Snapshot` へ書き込む。
///
/// 3フィールドを1関数にまとめることで、後から1つだけ書き忘れる事故
/// (特に `skipped_notifications`)を防ぐ。`AppHandle` を使わない純粋関数
/// なので、Tauri のモックアプリを用意しなくても単体テストできる
/// (`refresh.rs` の他の関数は `AppHandle` に依存し、現状テストハーネスが無い)。
///
/// **例外:** `commands.rs` の `mark_notification_read` はこの関数を経由せず
/// `snap.notifications` を直接書き換える(既読フラグだけの更新で、
/// `has_more_notifications` / `skipped_notifications` は変わらないため)。
/// つまりこの関数は「3フィールドをまとめて書く唯一の経路」ではなく、
/// 「まとめて書くときに1つ忘れる事故を防ぐ」ためのものに留まる。
fn apply_notification_page(
    s: &mut Snapshot,
    items: Vec<NotificationDto>,
    has_more: bool,
    skipped: usize,
) {
    s.notifications = items;
    s.has_more_notifications = has_more;
    s.skipped_notifications = skipped;
}

/// クライアント・設定・自分の情報・スペース情報をまとめて用意する。
/// me と space はキャッシュされ、初回だけ実際に取得する。
///
/// **戻り値の最後はセッション世代**(`AppState::session_generation`)。取得を
/// 始めた時点の値で、呼び出し側は状態へ書き戻す直前に
/// `AppState::session_changed` で照合する。取得中に接続が解除されると
/// `AppState` は初期化されるが、この関数を抜けた取得はもう資格情報を
/// 手元に持っているので、照合しないと初期化済みの状態へ結果を書き戻して
/// しまう(トレイの未読表示やトーストまで蘇る)。呼び出しごとに
/// `settings.is_some()` を見直すより、この1か所で世代を配る方が
/// 書き戻し地点を取りこぼしにくい。
async fn context(app: &AppHandle) -> Result<(Client, Settings, User, Space, u64), String> {
    let (generation, client, settings, me_cached, space_cached) = {
        let state = app.state::<AppState>();
        let cloned = (
            state.session_generation(),
            state.client.clone(),
            state.settings.lock().unwrap().clone(),
            state.me.lock().unwrap().clone(),
            state.space.lock().unwrap().clone(),
        );
        cloned
    };
    let settings =
        settings.ok_or_else(|| i18n::err_not_configured(app.state::<AppState>().lang()))?;
    let me = match me_cached {
        Some(me) => me,
        None => {
            let me = client.myself(&settings).await?;
            // キャッシュへの書き戻しも世代が変わっていたら行わない。
            // 呼び出し側の関門より前に `AppState` を汚さないため。
            let state = app.state::<AppState>();
            if !state.session_changed(generation) {
                *state.me.lock().unwrap() = Some(me.clone());
            }
            me
        }
    };
    // スペース名は不変なので初回だけ取得してキャッシュする
    let space = match space_cached {
        Some(space) => space,
        None => {
            let space = client.space(&settings).await?;
            let state = app.state::<AppState>();
            if !state.session_changed(generation) {
                *state.space.lock().unwrap() = Some(space.clone());
            }
            space
        }
    };
    Ok((client, settings, me, space, generation))
}

/// キャッシュ中の Snapshot を部分更新し、emit して更新後のクローンを返す。
fn emit_snapshot(
    app: &AppHandle,
    space_name: &str,
    update: impl FnOnce(&mut Snapshot),
) -> Snapshot {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut guard = state.snapshot.lock().unwrap();
        let mut snap = guard.take().unwrap_or_default();
        snap.space_name = space_name.to_string();
        update(&mut snap);
        *guard = Some(snap.clone());
        snap
    };
    let _ = app.emit("snapshot", &snapshot);
    snapshot
}

/// 現在のSnapshotをそのまま返す(取得をスキップした時用)。
fn current_snapshot(app: &AppHandle) -> Snapshot {
    app.state::<AppState>()
        .snapshot
        .lock()
        .unwrap()
        .clone()
        .unwrap_or_default()
}

/// 取得を始めてから接続セッションが変わっていたら `true`。
///
/// 各 fetch はこれが `true` の間、状態・トレイ・トーストに一切触れずに
/// 打ち切る。**`Err` ではなく「変化なし」として返す**のは、解除は利用者が
/// 意図した操作でありエラーではないため。`Err` を返すと `spawn_poller` が
/// `refresh-error` を emit し、解除直後の画面にエラーを出してしまう。
fn session_changed(app: &AppHandle, generation: u64) -> bool {
    app.state::<AppState>().session_changed(generation)
}

/// 通知を更新する。通知一覧(最大50件)は未読数が変化した時・初回・強制時だけ取得する。
pub(crate) async fn fetch_notifications(
    app: &AppHandle,
    notify: bool,
    force: bool,
) -> Result<Snapshot, String> {
    let (client, settings, _me, space, generation) = context(app).await?;
    let (prev_unread, store_empty) = {
        let state = app.state::<AppState>();
        let prev = state
            .snapshot
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.unread_count);
        let empty = state.notifications.lock().unwrap().items.is_empty();
        (prev, empty)
    };
    let unread_count = client.unread_count(&settings).await?;
    let need = force || store_empty || prev_unread != Some(unread_count);
    let page = if need {
        Some(client.notifications(&settings, None).await?)
    } else {
        None
    };
    // **ここから下がすべて書き戻し。** ネットワーク待ちの間に接続が解除
    // されていたら、トースト・ストア・トレイのどれにも触れずに打ち切る
    // (`last_notified_id` が 0 に戻っているので、そのまま進むと
    // 「未読がN件あります」の初回トーストまで出てしまう)。
    if session_changed(app, generation) {
        return Ok(current_snapshot(app));
    }
    if let Some(page) = page {
        if notify {
            notify_new_arrivals(app, &page.items, unread_count);
        } else {
            // トーストは出さないが、既知の通知として基準IDは進めておく
            let state = app.state::<AppState>();
            let mut last = state.last_notified_id.lock().unwrap();
            let max_id = page.items.iter().map(|n| n.id).max().unwrap_or(*last);
            if max_id > *last {
                *last = max_id;
            }
        }
        app.state::<AppState>()
            .notifications
            .lock()
            .unwrap()
            .merge(page);
    }
    let (items, has_more, skipped) = {
        let state = app.state::<AppState>();
        let store = state.notifications.lock().unwrap();
        (store.items.clone(), store.has_more(), store.skipped())
    };
    let snapshot = emit_snapshot(app, &space.name, |s| {
        apply_notification_page(s, items, has_more, skipped);
        s.unread_count = unread_count;
    });
    update_tray(app, unread_count);
    Ok(snapshot)
}

/// 担当している未完了課題を更新する。定期チェックは件数だけを取得し、一覧は
/// 件数が変化した時・初回・強制時(タブ表示や手動更新)だけ取得する。
pub(crate) async fn fetch_issues(app: &AppHandle, force: bool) -> Result<Snapshot, String> {
    let (client, settings, me, space, generation) = context(app).await?;
    let (prev, empty) = {
        let state = app.state::<AppState>();
        let prev = *state.last_issues_count.lock().unwrap();
        let empty = state
            .snapshot
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.issues.is_empty())
            .unwrap_or(true);
        (prev, empty)
    };
    let count = client.issues_count(&settings, me.id).await?;
    let need = force || empty || prev != Some(count);
    let issues = if need {
        Some(client.my_issues(&settings, me.id).await?)
    } else {
        None
    };
    // 書き戻しの手前に関門を置く(`fetch_notifications` と同じ理由)。
    // 件数の記録もここより後ろに置き、解除後の state を汚さない。
    if session_changed(app, generation) {
        return Ok(current_snapshot(app));
    }
    *app.state::<AppState>().last_issues_count.lock().unwrap() = Some(count);

    match issues {
        Some(issues) => Ok(emit_snapshot(app, &space.name, |s| {
            s.issues = issues;
            s.issue_count = count;
        })),
        // 件数に変化なし(=一覧も据え置き)。取得せず現在のSnapshotを返す。
        None => Ok(current_snapshot(app)),
    }
}

/// ウォッチを更新する。一覧(最大100件)は「更新ありウォッチ数」が変化した時・
/// 初回・強制時だけ取得する(count ゲート、通知と同じ方式)。
pub(crate) async fn fetch_watchings(app: &AppHandle, force: bool) -> Result<Snapshot, String> {
    let (client, settings, me, space, generation) = context(app).await?;
    let (prev, empty) = {
        let state = app.state::<AppState>();
        let prev = *state.last_watching_count.lock().unwrap();
        let empty = state
            .snapshot
            .lock()
            .unwrap()
            .as_ref()
            .map(|s| s.watchings.is_empty())
            .unwrap_or(true);
        (prev, empty)
    };
    let count = client.watching_count(&settings, me.id).await?;
    let need = force || empty || prev != Some(count);
    let watchings = if need {
        Some(client.watchings(&settings, me.id).await?)
    } else {
        None
    };
    // 書き戻しの手前に関門を置く(`fetch_notifications` と同じ理由)
    if session_changed(app, generation) {
        return Ok(current_snapshot(app));
    }
    *app.state::<AppState>().last_watching_count.lock().unwrap() = Some(count);

    match watchings {
        Some(watchings) => Ok(emit_snapshot(app, &space.name, |s| {
            s.watchings = watchings;
        })),
        // 変化なし: 取得せず、現在のSnapshotをそのまま返す
        None => Ok(current_snapshot(app)),
    }
}

/// 通知・課題・ウォッチをまとめて更新する(手動更新・初回用)。
pub(crate) async fn refresh_all(
    app: &AppHandle,
    notify: bool,
    force_notifications: bool,
) -> Result<Snapshot, String> {
    fetch_notifications(app, notify, force_notifications).await?;
    fetch_issues(app, force_notifications).await?;
    fetch_watchings(app, force_notifications).await
}

/// 指定間隔で `task` を実行し続けるバックグラウンドタスクを起動する。
/// 未設定(未接続)のときはスキップし、失敗時は refresh-error を emit する。
///
/// **この `is_some()` はティックの開始を止めるだけ。** 走り出した取得は
/// 資格情報を手元に持っているので、途中で接続が解除されてもここでは
/// 止まらない。その取りこぼしはセッション世代(`context` 参照)で防ぐ。
pub(crate) fn spawn_poller<F, Fut>(app: AppHandle, interval_secs: u64, task: F)
where
    F: Fn(AppHandle) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<Snapshot, String>> + Send,
{
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(interval_secs));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let configured = app.state::<AppState>().settings.lock().unwrap().is_some();
            if configured {
                if let Err(e) = task(app.clone()).await {
                    let _ = app.emit("refresh-error", e);
                }
            }
        }
    });
}

/// さらに古い通知を1ページ取得してストアへ追加する
pub(crate) async fn load_older(app: &AppHandle) -> Result<Snapshot, String> {
    let (client, settings, max_id) = {
        let state = app.state::<AppState>();
        let cloned = (
            state.client.clone(),
            state.settings.lock().unwrap().clone(),
            state.notifications.lock().unwrap().min_id(),
        );
        cloned
    };
    let settings =
        settings.ok_or_else(|| i18n::err_not_configured(app.state::<AppState>().lang()))?;

    // 現在の最古IDより古いものを取得(ストアが空なら最新ページ相当)
    let older = client.notifications(&settings, max_id).await?;

    let state = app.state::<AppState>();
    {
        let mut store = state.notifications.lock().unwrap();
        // 古いページ用のマージ。新規0件なら打ち止めにする判断は
        // `merge_older` の中にある(そこで単体テストできるため)。
        store.merge_older(older);
    }
    // 通知以外(未読数・課題・ウォッチ)は直近のSnapshotの値を引き継ぐ
    let snapshot = {
        let store = state.notifications.lock().unwrap();
        let mut guard = state.snapshot.lock().unwrap();
        let mut snapshot = guard.clone().unwrap_or_default();
        apply_notification_page(
            &mut snapshot,
            store.items.clone(),
            store.has_more(),
            store.skipped(),
        );
        *guard = Some(snapshot.clone());
        snapshot
    };
    let _ = app.emit("snapshot", &snapshot);
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::test_notification_dto as dto;

    /// `apply_notification_page` が3フィールドとも書き込むこと。
    ///
    /// `refresh.rs` は `AppHandle` に依存するためモックアプリなしにテスト
    /// できない関数が大半だが、`skipped_notifications` を書き込む代入
    /// (`fetch_notifications` / `load_older` の両方が使う)はこの関数に
    /// 抜き出してあるので、ここで直接固定できる。**この抜き出し以前は、
    /// この代入を消す変異がテストで検知できなかった。**
    #[test]
    fn apply_notification_page_sets_all_three_fields() {
        let mut snapshot = Snapshot::default();
        apply_notification_page(&mut snapshot, vec![dto(1), dto(2)], true, 4);
        assert_eq!(
            snapshot
                .notifications
                .iter()
                .map(|n| n.id)
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(snapshot.has_more_notifications);
        assert_eq!(snapshot.skipped_notifications, 4);
    }

    /// 既存のSnapshotに上書きする形でも、他のフィールド(例: unread_count)を
    /// 巻き込まずに3フィールドだけ更新すること。
    #[test]
    fn apply_notification_page_leaves_other_fields_untouched() {
        let mut snapshot = Snapshot {
            unread_count: 9,
            issue_count: 3,
            ..Snapshot::default()
        };
        apply_notification_page(&mut snapshot, vec![], false, 0);
        assert_eq!(snapshot.unread_count, 9);
        assert_eq!(snapshot.issue_count, 3);
        assert_eq!(snapshot.skipped_notifications, 0);
    }
}
