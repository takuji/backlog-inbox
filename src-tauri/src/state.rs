//! アプリ全体で共有する状態(AppState)と、その構成要素。

use crate::backlog::{
    Client, NotificationDto, NotificationPage, Settings, Snapshot, Space, User, NOTIF_PAGE,
};
use crate::i18n::Lang;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::image::Image;

/// メモリ上に蓄積する通知の上限件数
const NOTIF_CAP: usize = 500;

/// 蓄積した通知のストア。アプリ内で唯一の真実の源とし、ポーリングと
/// 「もっと見る」の両方がここへマージする(永続化はしない)
#[derive(Default)]
pub(crate) struct NotifStore {
    pub(crate) items: Vec<NotificationDto>,
    /// これ以上古い通知はサーバーに存在しないと判明したか
    reached_end: bool,
    /// **直近の取得で**読めなかった件数。累計にすると、ポーリングが毎回
    /// 1ページ目を取り直すので同じ要素を数え続けて無限に増える。
    skipped: usize,
}

impl NotifStore {
    /// 取得した通知をID重複を排除しつつマージし、新しい順・上限件数に整える。
    /// 取得ページが満たない場合は「これで最後」と判断する。
    ///
    /// 戻り値は、このページで新たに増えた(=マージ前のストアに無かったIDの)
    /// 件数。ポーリングは毎回1ページ目を取り直すので、ここが0になるのは
    /// 「変化なし」という普通の結果であり、それだけでは「もう続きが無い」と
    /// 判断できない。カーソルを進める「もっと見る」側(`refresh.rs` の
    /// `load_older`)は [`Self::merge_older`] を使い、そちらがこの値を見て
    /// 打ち止めを判断する。
    pub(crate) fn merge(&mut self, page: NotificationPage) -> usize {
        // **スキップされた件も「サーバーから返ってきた件数」に数える。**
        // items.len() だけで判定すると、1件落ちただけで 49 < 50 となり、
        // まだ続きがあるのにページングが止まる。
        if page.items.len() + page.skipped < NOTIF_PAGE {
            self.reached_end = true;
        }
        self.skipped = page.skipped;
        let existing = std::mem::take(&mut self.items);
        let mut map: HashMap<i64, NotificationDto> = HashMap::new();
        for n in existing {
            map.insert(n.id, n);
        }
        // 取得したものを優先(既読状態など最新の内容で上書き)。
        // 既に無かったIDだけを「新規」として数える。
        let mut added = 0usize;
        for n in page.items {
            if !map.contains_key(&n.id) {
                added += 1;
            }
            map.insert(n.id, n);
        }
        let mut items: Vec<NotificationDto> = map.into_values().collect();
        items.sort_by_key(|b| std::cmp::Reverse(b.id));
        items.truncate(NOTIF_CAP);
        self.items = items;
        added
    }

    pub(crate) fn min_id(&self) -> Option<i64> {
        self.items.last().map(|n| n.id)
    }

    pub(crate) fn has_more(&self) -> bool {
        !self.reached_end && self.items.len() < NOTIF_CAP
    }

    pub(crate) fn skipped(&self) -> usize {
        self.skipped
    }

    /// クリックで開いた通知を「リソース閲覧済み」として記録する。
    /// 見つからなければ何もせず `false`。
    ///
    /// サーバーは `/globalbar/notifications/redirect/{id}` が開かれた時点で
    /// `resourceAlreadyRead` を立てるが、その事実がこちらに届くのは次に
    /// **その通知を含むページ**を取得したときだけ。更新が取り直すのは
    /// 1ページ目なので、「もっと見る」で遡った通知は最初に取得したときの値で
    /// 凍結され、未閲覧の絞り込みに残り続ける。ここで記録しておけば、
    /// [`Self::merge`] は取得に含まれる分しか上書きしないため残る。
    pub(crate) fn mark_resource_viewed(&mut self, id: i64) -> bool {
        match self.items.iter_mut().find(|n| n.id == id) {
            Some(n) => {
                n.resource_already_read = true;
                true
            }
            None => false,
        }
    }

    /// 「もっと見る」で取得した古いページをマージする。
    ///
    /// 新しい要素が1件も増えなければ、そこで打ち止めにする。カーソル
    /// (`min_id`)は増えた要素からしか進まないので、増えなかった場合は
    /// 同じ要求を繰り返すだけになり、ボタンが永久に空振りする。
    /// ページ全件が読めなかったときに起きる。
    ///
    /// **定期ポーリング(1ページ目の取り直し)では [`Self::merge`] を使うこと。**
    /// あちらは既知の要素しか返らないのが正常なので、同じ判断をすると
    /// すぐにページングが止まってしまう。
    pub(crate) fn merge_older(&mut self, page: NotificationPage) {
        if self.merge(page) == 0 {
            self.reached_end = true;
        }
    }
}

pub(crate) struct AppState {
    pub(crate) client: Client,
    pub(crate) settings: Mutex<Option<Settings>>,
    pub(crate) me: Mutex<Option<User>>,
    pub(crate) space: Mutex<Option<Space>>,
    pub(crate) last_notified_id: Mutex<i64>,
    pub(crate) notifications: Mutex<NotifStore>,
    pub(crate) snapshot: Mutex<Option<Snapshot>>,
    // 表示言語。Client とも共有し、set_language で更新する。
    pub(crate) lang: Arc<Mutex<Lang>>,
    // メニューバーに未読件数を数字表示するか
    pub(crate) show_unread_count: Mutex<bool>,
    // ユーザーアイコンの data URI キャッシュ(userId -> data URI)
    pub(crate) icon_cache: Mutex<HashMap<i64, String>>,
    // スペースロゴの data URI キャッシュ
    pub(crate) space_logo: Mutex<Option<String>>,
    // 直近の「更新ありウォッチ数」。ウォッチ一覧の取得ゲートに使う。
    pub(crate) last_watching_count: Mutex<Option<i64>>,
    // 直近の担当課題数。課題一覧の取得ゲートに使う。
    pub(crate) last_issues_count: Mutex<Option<i64>>,
    // MCPサーバーを有効にするか(prefsの値。ユーザーの意図)
    pub(crate) mcp_enabled: Mutex<bool>,
    // 組み込みMCPサーバーのハンドル(Some=稼働中)
    pub(crate) mcp_server: Mutex<Option<backlog_mcp::HttpServerHandle>>,
    // 直近のMCPサーバー起動エラー
    pub(crate) mcp_error: Mutex<Option<String>>,
    // 接続セッションの世代。`reset_session` のたびに進む。取得中に接続が
    // 切り替わったことを、進行中のポーリング側から検知するための目印
    // (照合しているのは `refresh.rs`)。
    pub(crate) session_generation: AtomicU64,
}

impl AppState {
    pub(crate) fn lang(&self) -> Lang {
        *self.lang.lock().unwrap()
    }

    /// いま進行中の接続セッションを表す番号。
    ///
    /// ネットワーク取得を始める前にこれを控えておき、取得結果を状態へ
    /// 書き戻す直前に [`Self::session_changed`] で確かめる。
    pub(crate) fn session_generation(&self) -> u64 {
        self.session_generation.load(Ordering::SeqCst)
    }

    /// 控えておいた世代から接続セッションが変わっていれば `true`。
    pub(crate) fn session_changed(&self, generation: u64) -> bool {
        self.session_generation() != generation
    }

    /// 接続に紐づく状態をすべて初期状態に戻す。
    ///
    /// 接続情報の保存(`save_settings`)と解除(`clear_settings`)の両方が使う。
    /// フィールドをべた書きで並べると、`AppState` に項目が増えたときに
    /// 片方だけ直して他方を忘れる事故が起きるため、1か所にまとめる。
    ///
    /// **prefs 由来の値(`lang` / `show_unread_count` / `mcp_enabled`)は触らない。**
    /// 接続を切ることは表示設定の変更意図ではない。`mcp_error` も対象外で、
    /// そのクリアは MCP サーバーを止める `clear_settings` の責務。
    pub(crate) fn reset_session(&self) {
        // **世代を先に進める。** 進行中の取得は書き戻す直前にここを照合
        // するので、先に進めておけば、リセットと同時に判定した取得は
        // すべて「中断する」側に倒れる。
        self.session_generation.fetch_add(1, Ordering::SeqCst);
        *self.settings.lock().unwrap() = None;
        *self.me.lock().unwrap() = None;
        *self.space.lock().unwrap() = None;
        *self.last_notified_id.lock().unwrap() = 0;
        *self.notifications.lock().unwrap() = NotifStore::default();
        *self.snapshot.lock().unwrap() = None;
        self.icon_cache.lock().unwrap().clear();
        *self.space_logo.lock().unwrap() = None;
        *self.last_watching_count.lock().unwrap() = None;
        *self.last_issues_count.lock().unwrap() = None;
    }
}

pub(crate) struct TrayIcons {
    pub(crate) normal: Image<'static>,
    pub(crate) unread: Image<'static>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backlog::test_notification_dto as dto;

    fn page(ids: &[i64], skipped: usize) -> NotificationPage {
        NotificationPage {
            items: ids.iter().map(|i| dto(*i)).collect(),
            skipped,
        }
    }

    /// **1件落ちてもページングが止まらないこと。**
    ///
    /// 判定を `items.len()` だけで行うと、49件+1スキップで
    /// 49 < NOTIF_PAGE(50) となり、まだ続きがあるのに「最後まで来た」と
    /// 誤判定してページングが静かに止まる。
    #[test]
    fn a_skipped_item_does_not_stop_pagination() {
        let ids: Vec<i64> = (1..=(NOTIF_PAGE as i64 - 1)).collect();
        let mut store = NotifStore::default();
        store.merge(page(&ids, 1));
        assert!(
            store.has_more(),
            "1件スキップされただけでページングが止まっている"
        );
    }

    /// 取得件数が本当に足りなければ、これで最後と判断すること。
    #[test]
    fn a_short_page_still_ends_pagination() {
        let ids: Vec<i64> = (1..=(NOTIF_PAGE as i64 - 2)).collect();
        let mut store = NotifStore::default();
        store.merge(page(&ids, 1));
        assert!(!store.has_more());
    }

    /// ちょうど1ページ分あれば続きがあると判断すること。
    #[test]
    fn a_full_page_continues_pagination() {
        let ids: Vec<i64> = (1..=(NOTIF_PAGE as i64)).collect();
        let mut store = NotifStore::default();
        store.merge(page(&ids, 0));
        assert!(store.has_more());
    }

    /// **スキップ件数は累計ではなく直近の取得の値であること。**
    ///
    /// ポーリングは毎回1ページ目を取り直すので、累計にすると同じ壊れた
    /// 要素を数え続けて数字が無限に増える。
    #[test]
    fn skipped_reflects_the_latest_fetch_only() {
        let mut store = NotifStore::default();
        store.merge(page(&[1, 2], 3));
        assert_eq!(store.skipped(), 3);
        store.merge(page(&[1, 2], 3));
        assert_eq!(store.skipped(), 3, "同じページを2回取って倍になっている");
        store.merge(page(&[1, 2], 0));
        assert_eq!(store.skipped(), 0, "正常な取得の後も古い値が残っている");
    }

    /// 全件が既にストアにあるページをマージしても、新規0件と報告すること
    /// (ポーリングが1ページ目を取り直す通常のケース。finding 1)。
    #[test]
    fn merging_only_already_known_items_reports_zero_new() {
        let mut store = NotifStore::default();
        store.merge(page(&[1, 2, 3], 0));
        let added = store.merge(page(&[1, 2, 3], 0));
        assert_eq!(added, 0);
    }

    /// 新規と既知が混ざったページでは、新規分の件数だけを報告すること。
    #[test]
    fn merging_a_mix_reports_only_the_new_count() {
        let mut store = NotifStore::default();
        store.merge(page(&[1, 2], 0));
        let added = store.merge(page(&[2, 3, 4], 0));
        assert_eq!(added, 2, "既知の2は数えず、新規の3・4だけを数える");
    }

    /// 古いページが1件も新しい要素を増やさなければ、そこで打ち止めになること。
    ///
    /// **この判断が無いと「もっと見る」が永久に空振りする**: ページ全件が
    /// 読めないと items が増えず `min_id` も動かないので、同じページを
    /// 取り続けることになる。
    #[test]
    fn merge_older_stops_when_the_page_adds_nothing() {
        let ids: Vec<i64> = (1..=(NOTIF_PAGE as i64)).collect();
        let mut store = NotifStore::default();
        store.merge(page(&ids, 0));
        assert!(store.has_more(), "前提: このページ単体では続きがある");

        // 全件が読めなかった古いページ(items 0件、skipped が満杯)
        store.merge_older(page(&[], NOTIF_PAGE));
        assert!(
            !store.has_more(),
            "新しい要素が増えていないのにページングが続いている"
        );
    }

    /// クリックで開いた通知を「リソース閲覧済み」として記録できること。
    #[test]
    fn mark_resource_viewed_sets_the_flag() {
        let mut store = NotifStore::default();
        store.merge(page(&[1, 2], 0));
        assert!(!store.items.iter().any(|n| n.resource_already_read));

        assert!(store.mark_resource_viewed(2));

        let marked = store.items.iter().find(|n| n.id == 2).unwrap();
        assert!(marked.resource_already_read);
        let untouched = store.items.iter().find(|n| n.id == 1).unwrap();
        assert!(!untouched.resource_already_read);
    }

    /// ストアに無いIDは何もせず false を返すこと(呼び出し側は握りつぶす)。
    #[test]
    fn mark_resource_viewed_ignores_an_unknown_id() {
        let mut store = NotifStore::default();
        store.merge(page(&[1], 0));
        assert!(!store.mark_resource_viewed(999));
    }

    /// **記録が、そのIDを含まない取得の後も残ること。**
    ///
    /// 更新ボタンが取り直すのは1ページ目だけなので、「もっと見る」で遡った
    /// 通知はマージの対象に入らない。ここが消えると、サーバー側では閲覧済みに
    /// なっているのにストアだけが最初に取得したときの値で凍結され、未閲覧の
    /// 絞り込みに残り続ける(暗転しているのに未閲覧、という状態になる)。
    #[test]
    fn mark_resource_viewed_survives_a_merge_that_omits_the_id() {
        let mut store = NotifStore::default();
        store.merge(page(&[10, 11], 0));
        store.mark_resource_viewed(10);

        // 1ページ目の取り直し。古い10は含まれない
        store.merge(page(&[11, 12], 0));

        let old = store.items.iter().find(|n| n.id == 10).unwrap();
        assert!(
            old.resource_already_read,
            "取り直しに含まれない古い通知の記録が消えている"
        );
    }

    /// 古いページが新しい要素を増やしたなら、まだ続きがあると判断すること。
    #[test]
    fn merge_older_continues_when_the_page_adds_items() {
        let first: Vec<i64> = (101..=(100 + NOTIF_PAGE as i64)).collect();
        let older: Vec<i64> = (1..=(NOTIF_PAGE as i64)).collect();
        let mut store = NotifStore::default();
        store.merge(page(&first, 0));
        store.merge_older(page(&older, 0));
        assert!(store.has_more());
    }

    /// 接続系フィールドを「汚れた」状態にした `AppState` を作る。
    ///
    /// `AppState` は `AppHandle` 無しで組み立てられる(`Client::new` は
    /// `Arc<Mutex<Lang>>` だけを取り、他は素の `Mutex`、`mcp_server` は `None`)。
    fn dirty_state() -> AppState {
        let lang = Arc::new(Mutex::new(Lang::En));
        let mut icons = HashMap::new();
        icons.insert(7, "data:image/png;base64,x".to_string());
        let mut store = NotifStore::default();
        store.merge(page(&[1, 2], 3));
        AppState {
            client: Client::new(lang.clone()),
            settings: Mutex::new(Some(Settings {
                space_host: "example.backlog.com".to_string(),
                api_key: "k".to_string(),
            })),
            me: Mutex::new(Some(User {
                id: 1,
                name: "me".to_string(),
            })),
            space: Mutex::new(Some(Space {
                name: "space".to_string(),
            })),
            last_notified_id: Mutex::new(42),
            notifications: Mutex::new(store),
            snapshot: Mutex::new(Some(Snapshot {
                unread_count: 5,
                ..Snapshot::default()
            })),
            lang,
            show_unread_count: Mutex::new(true),
            icon_cache: Mutex::new(icons),
            space_logo: Mutex::new(Some("data:image/png;base64,y".to_string())),
            last_watching_count: Mutex::new(Some(3)),
            last_issues_count: Mutex::new(Some(4)),
            mcp_enabled: Mutex::new(true),
            mcp_server: Mutex::new(None),
            mcp_error: Mutex::new(Some("boom".to_string())),
            // 0 以外から始めて、「常に 0 と比較している」実装では
            // 世代のテストが通らないようにしておく。
            session_generation: AtomicU64::new(7),
        }
    }

    /// 接続に紐づく10フィールドがすべて初期状態に戻ること。
    #[test]
    fn reset_session_clears_the_connection_state() {
        let state = dirty_state();
        state.reset_session();

        assert!(state.settings.lock().unwrap().is_none());
        assert!(state.me.lock().unwrap().is_none());
        assert!(state.space.lock().unwrap().is_none());
        assert_eq!(*state.last_notified_id.lock().unwrap(), 0);
        assert!(state.snapshot.lock().unwrap().is_none());
        assert!(state.icon_cache.lock().unwrap().is_empty());
        assert!(state.space_logo.lock().unwrap().is_none());
        assert!(state.last_watching_count.lock().unwrap().is_none());
        assert!(state.last_issues_count.lock().unwrap().is_none());

        let store = state.notifications.lock().unwrap();
        assert!(store.items.is_empty());
        assert_eq!(store.skipped(), 0);
        // 中身を空にするだけでなくストアごと作り直すこと。汚れた側は
        // 2件+3スキップ < 50 で打ち止め済みなので、has_more が true に
        // 戻ることが「作り直した」証拠になる。
        assert!(store.has_more());
    }

    /// **prefs 由来の値と `mcp_error` は触らないこと。**
    ///
    /// 接続を切ることは表示設定の変更意図ではない。`mcp_error` のクリアは
    /// MCP サーバーを止める `clear_settings` の責務で、ここではやらない。
    ///
    /// **`mcp_server` も対象外だが、それはここで検証できない。**
    /// `reset_session` がハンドルを落とすと `save_settings` から呼んだとき
    /// に稼働中のサーバーを黙って捨ててしまうため、意図的に触っていない。
    /// ただし `backlog_mcp::HttpServerHandle` はテストから作れず、
    /// `dirty_state()` でも `None` にしかできないので、「触っていない」と
    /// 「リセットされた」を区別できない。**この行の不在をテストの穴と
    /// 誤解して「直さない」こと。**
    #[test]
    fn reset_session_keeps_prefs_and_mcp_error() {
        let state = dirty_state();
        state.reset_session();

        assert_eq!(state.lang(), Lang::En);
        assert!(*state.show_unread_count.lock().unwrap());
        assert!(*state.mcp_enabled.lock().unwrap());
        assert_eq!(state.mcp_error.lock().unwrap().as_deref(), Some("boom"));
    }

    /// **`reset_session` がセッション世代を進めること。**
    ///
    /// 進行中のポーリングは取得開始時の世代を控えておき、状態へ書き戻す
    /// 直前に照合する。世代が進まないと、解除の直後に完了した取得が
    /// 初期化済みの状態を書き戻し、トレイの未読表示やトーストを蘇らせる。
    #[test]
    fn reset_session_bumps_the_session_generation() {
        let state = dirty_state();
        let before = state.session_generation();

        state.reset_session();

        assert_ne!(state.session_generation(), before);
        assert!(
            state.session_changed(before),
            "解除前に控えた世代が有効なままになっている"
        );
    }

    /// リセットしていない間は世代が変わらないこと。正常な取得まで
    /// 中断してしまわないための裏返しの確認。
    #[test]
    fn the_session_generation_is_stable_without_a_reset() {
        let state = dirty_state();
        let generation = state.session_generation();

        assert!(!state.session_changed(generation));
        assert_eq!(state.session_generation(), generation);
    }

    /// 解除を繰り返しても毎回新しい世代になること。固定値を代入する
    /// 実装(例: 常に 1 にする)では2回目以降の解除を検知できない。
    #[test]
    fn each_reset_yields_a_new_generation() {
        let state = dirty_state();
        let first = state.session_generation();
        state.reset_session();
        let second = state.session_generation();
        state.reset_session();
        let third = state.session_generation();

        assert_ne!(first, second);
        assert_ne!(second, third);
    }
}
