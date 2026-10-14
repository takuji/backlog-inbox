import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import * as api from "./api";
import { useI18n } from "./i18n";
import { SettingsForm } from "./components/SettingsForm";
import { NotificationList } from "./components/NotificationList";
import { IssueList } from "./components/IssueList";
import { WatchingList } from "./components/WatchingList";
import { SpaceLogo } from "./components/SpaceLogo";
import {
  NotificationFilter,
  emptyFilter,
  isFilterActive,
  type NotifFilter,
} from "./components/NotificationFilter";
import "./App.css";

type Tab = "notifications" | "issues" | "watchings";

function TabIcon({ tab }: { tab: Tab }) {
  const paths = {
    notifications: (
      <>
        <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
        <path d="M13.73 21a2 2 0 0 1-3.46 0" />
      </>
    ),
    issues: (
      <>
        <polyline points="9 11 12 14 22 4" />
        <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
      </>
    ),
    watchings: (
      <>
        <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
        <circle cx="12" cy="12" r="3" />
      </>
    ),
  } as const;
  return (
    <svg
      width="20"
      height="20"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {paths[tab]}
    </svg>
  );
}

export default function App() {
  const { t, lang } = useI18n();
  const [loaded, setLoaded] = useState(false);
  const [settings, setSettings] = useState<api.Settings | null>(null);
  const [showSettings, setShowSettings] = useState(false);
  const [snapshot, setSnapshot] = useState<api.Snapshot | null>(null);
  const [tab, setTab] = useState<Tab>("notifications");
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [updatedAt, setUpdatedAt] = useState<Date | null>(null);
  const [focused, setFocused] = useState(() => document.hasFocus());
  // このセッションでクリックして開いた通知のID。
  //
  // **Snapshot に直接書き込んではいけない。** バックエンドは
  // `resource_already_read` に true を書く経路を持たず(APIの値をそのまま
  // 写すだけ)、`snapshot` イベントは通知配列を丸ごと差し替える。Snapshot 側に
  // 印を付けても、課題タブを開いたときの取得など次の emit ひとつで消える。
  // ここに別に持って描画時に被せることで、何度 snapshot が届いても残る。
  const [viewedIds, setViewedIds] = useState<Set<number>>(() => new Set());
  // 通知一覧の絞り込み条件。**永続化しない。**
  //
  // トレイ常駐で何度も開き直すアプリで条件が残ると、通知が届いているのに
  // 条件から外れていて気づかない、という最も起こしてはいけない事故になる。
  // 起動するたび全件から始める。
  const [filter, setFilter] = useState<NotifFilter>(emptyFilter);

  const unread = snapshot?.unreadCount ?? 0;

  // **生の通知 → 絞り込み → viewedIds の被せ、の順で流す。**
  //
  // 順序が重要。被せた後の値で絞ると、クリックした瞬間にその通知が一覧から
  // 消えて下の項目が繰り上がる。クリックはブラウザーでページを開く操作なので、
  // 続けてクリックすると意図しないページを開く事故になる。生の値で絞れば、
  // クリックした通知はその場に残って暗くなるだけで済む。
  const matched = useMemo(() => {
    const items = snapshot?.notifications ?? [];
    return items.filter(
      (n) =>
        (filter.senderId === null || n.senderId === filter.senderId) &&
        (filter.reason === null || n.reason === filter.reason) &&
        (!filter.unviewedOnly || !n.resourceAlreadyRead)
    );
  }, [snapshot?.notifications, filter]);

  // クリック済みの通知に「リソース閲覧済み」を被せる(表示のみ)。
  const notifications = useMemo(
    () =>
      matched.map((n) =>
        viewedIds.has(n.id) ? { ...n, resourceAlreadyRead: true } : n
      ),
    [matched, viewedIds]
  );

  // 選択肢は**絞り込み後ではなく生の一覧**から作る。絞り込み後から作ると、
  // 一度選んだ瞬間に他の選択肢が消えて選び直せなくなる。
  const senders = useMemo(() => {
    const map = new Map<number, string>();
    for (const n of snapshot?.notifications ?? []) {
      // 一覧は新しい順なので、**最初に出てきた名前を採る**。素直に上書きすると
      // 配列の最後＝最も古い出現が残り、改名した相手が旧姓で並び続ける。
      if (n.senderId !== null && !map.has(n.senderId)) map.set(n.senderId, n.senderName);
    }
    // **選択中の送信者は、一覧から消えても選択肢に残す。** 一致する option が
    // 無い select は仕様上どれも選択されず空欄で描かれる。条件は解除しない
    // 方針なので、そのままでは「何かで絞られている」ことしか分からず、
    // 何で絞られているのかが見えず選び直すこともできなくなる。
    // 上の !has と併せて、まだ居る送信者は取得し直した新しい名前が勝つ。
    if (filter.senderId !== null && !map.has(filter.senderId)) {
      map.set(filter.senderId, filter.senderName ?? String(filter.senderId));
    }
    return [...map]
      .map(([id, name]) => ({ id, name }))
      .sort((a, b) => a.name.localeCompare(b.name));
  }, [snapshot?.notifications, filter.senderId, filter.senderName]);

  const reasons = useMemo(() => {
    const set = new Set((snapshot?.notifications ?? []).map((n) => n.reason));
    // 送信者と同じ理由で選択中の種別を合流させる。こちらは `t.reason()` が
    // 任意のコードを表示できるので、コードを足すだけでよい。
    if (filter.reason !== null) set.add(filter.reason);
    return [...set].sort((a, b) => a - b);
  }, [snapshot?.notifications, filter.reason]);

  const doRefresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      const snap = await api.refresh();
      setSnapshot(snap);
      setUpdatedAt(new Date());
    } catch (e) {
      setError(String(e));
    } finally {
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    (async () => {
      const s = await api.getSettings();
      if (disposed) return;
      setSettings(s);
      setShowSettings(!s);
      const snap = await api.getSnapshot();
      if (disposed) return;
      if (snap) {
        setSnapshot(snap);
        setUpdatedAt(new Date());
      } else if (s) {
        void doRefresh();
      }
      setLoaded(true);
    })();

    const unSnapshot = listen<api.Snapshot>("snapshot", (e) => {
      setSnapshot(e.payload);
      setUpdatedAt(new Date());
      setError(null);
    });
    const unError = listen<string>("refresh-error", (e) => setError(e.payload));
    return () => {
      disposed = true;
      unSnapshot.then((f) => f());
      unError.then((f) => f());
    };
  }, [doRefresh]);

  // ウィンドウが前面にあるか(ユーザーが実際に見ているか)を追跡する。
  // 隠れて常駐している間に届いた通知まで自動既読にしないための判定に使う。
  useEffect(() => {
    const onFocus = () => setFocused(true);
    const onBlur = () => setFocused(false);
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
    };
  }, []);

  // 接続先が変わったらクリック履歴も絞り込み条件も持ち越さない。通知IDも
  // 送信者IDもスペースごとの連番なので、繋ぎ直した先で同じIDの別人・別通知に
  // 出会う。開いてもいない通知が閲覧済みとして暗転したり、意図しない相手で
  // 絞られたままになる。
  useEffect(() => {
    setViewedIds(new Set());
    setFilter(emptyFilter);
  }, [settings?.spaceHost]);

  // 通知タブを実際に見ている(ウィンドウが前面 + 通知タブが開いている)ときは、
  // ユーザー操作なしで未読をすべて既読にしてバッジをクリアする。
  useEffect(() => {
    if (tab !== "notifications" || !focused || unread <= 0) return;
    (async () => {
      try {
        const snap = await api.markAllRead();
        setSnapshot(snap);
        setUpdatedAt(new Date());
      } catch {
        // 自動既読化の失敗は致命的ではないので黙って無視する
      }
    })();
  }, [tab, focused, unread]);

  // 担当課題タブを開いたら最新化する(件数ゲートで拾えない中身の変化に対応)。
  // 短時間の行き来で叩きすぎないよう軽くスロットルする。
  const lastIssuesRefresh = useRef(0);
  useEffect(() => {
    if (tab !== "issues") return;
    const now = Date.now();
    if (now - lastIssuesRefresh.current < 15000) return;
    lastIssuesRefresh.current = now;
    api.refreshIssues().catch(() => {});
  }, [tab]);

  const loadMore = async () => {
    setLoadingMore(true);
    try {
      const snap = await api.loadOlderNotifications();
      setSnapshot(snap);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingMore(false);
    }
  };

  const openNotification = (n: api.NotificationItem) => {
    void api.openUrl(n.url);
    if (!n.alreadyRead) {
      api.markNotificationRead(n.id).catch(() => {});
    }
    // 対象リソースを開いたので、クライアント側の閲覧済み状態を更新する(表示のみ)
    setViewedIds((prev) => new Set(prev).add(n.id));
    // ストアにも記録する。サーバーは redirect を開いた時点で
    // resourceAlreadyRead を立てるが、更新が取り直すのは1ページ目だけなので、
    // 「もっと見る」で遡った通知にはその値が永久に届かない。記録しておかないと
    // 暗転しているのに未閲覧の絞り込みに残り続ける。
    api.markResourceViewed(n.id).catch(() => {});
  };

  if (!loaded) return null;

  if (showSettings || !settings) {
    return (
      <SettingsForm
        // **接続状態が変わったらフォームごと作り直す。** `SettingsForm` は
        // `initial` をマウント時に一度だけローカル state へ写す。key が無いと
        // React は同じインスタンスを使い回すため、解除しても入力欄には
        // 削除済みの API キーが残り、そのまま保存すれば消したはずの鍵で
        // 再接続してしまう。`autoFocus` も同じ理由で効かなくなる。
        key={settings ? "connected" : "new"}
        initial={settings}
        onSaved={async () => {
          setSettings(await api.getSettings());
          setShowSettings(false);
        }}
        onCleared={() => {
          // 未接続に戻す。settings が null になることで
          // `showSettings || !settings` によりこの画面に留まり、
          // onCancel が undefined になって閉じるボタンとMCPセクションが消える
          setSettings(null);
          setSnapshot(null);
          setUpdatedAt(null);
          setError(null);
        }}
        onCancel={settings ? () => setShowSettings(false) : undefined}
      />
    );
  }

  const issueCount = snapshot?.issueCount ?? 0;
  const watchUpdated = snapshot?.watchings.filter((w) => w.updated).length ?? 0;

  return (
    <div className="app">
      <header className="header">
        {snapshot?.spaceName ? (
          <button
            className="brand brand-button"
            onClick={() => void api.openUrl(`https://${settings.spaceHost}/`)}
            title={t.openInBrowser}
          >
            <SpaceLogo key={snapshot.spaceName} />
            <h1>{snapshot.spaceName}</h1>
          </button>
        ) : (
          <div className="brand">
            <h1>Backlog Inbox</h1>
          </div>
        )}
        <div className="header-actions">
          {updatedAt && (
            <span className="updated-at">
              {t.updatedAt(
                updatedAt.toLocaleTimeString(lang === "ja" ? "ja-JP" : "en-US", {
                  hour: "2-digit",
                  minute: "2-digit",
                })
              )}
            </span>
          )}
          <button
            className="icon-button"
            onClick={() => void doRefresh()}
            disabled={refreshing}
            title={t.refreshTitle}
          >
            <svg
              className={refreshing ? "spin" : ""}
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <polyline points="23 4 23 10 17 10" />
              <polyline points="1 20 1 14 7 14" />
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" />
            </svg>
          </button>
          <button className="icon-button" onClick={() => setShowSettings(true)} title={t.settingsTitle}>
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden="true"
            >
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
          </button>
        </div>
      </header>

      {error && <div className="error-banner">{error}</div>}

      <div className="app-body">
        <nav className="sidebar">
          <button
            className={`side-tab ${tab === "notifications" ? "active" : ""}`}
            onClick={() => setTab("notifications")}
          >
            <TabIcon tab="notifications" />
            <span>{t.tabNotifications}</span>
            {unread > 0 && <span className="badge badge-red">{unread}</span>}
          </button>
          <button
            className={`side-tab ${tab === "issues" ? "active" : ""}`}
            onClick={() => setTab("issues")}
          >
            <TabIcon tab="issues" />
            <span>{t.tabIssues}</span>
            {issueCount > 0 && <span className="badge">{issueCount}</span>}
          </button>
          <button
            className={`side-tab ${tab === "watchings" ? "active" : ""}`}
            onClick={() => setTab("watchings")}
          >
            <TabIcon tab="watchings" />
            <span>{t.tabWatchings}</span>
            {watchUpdated > 0 && <span className="badge badge-blue">{watchUpdated}</span>}
          </button>
        </nav>

        <main className="content">
          {!snapshot ? (
            <div className="empty">{refreshing ? t.loading : t.noData}</div>
          ) : tab === "notifications" ? (
            <>
              <NotificationFilter
                senders={senders}
                reasons={reasons}
                value={filter}
                total={snapshot.notifications.length}
                shown={matched.length}
                onChange={setFilter}
                onClear={() => setFilter(emptyFilter)}
              />
              <NotificationList
                items={notifications}
                hasMore={snapshot.hasMoreNotifications}
                loadingMore={loadingMore}
                skipped={snapshot.skippedNotifications}
                filterActive={isFilterActive(filter)}
                onOpen={openNotification}
                onLoadMore={() => void loadMore()}
              />
            </>
          ) : tab === "issues" ? (
            <IssueList
              items={snapshot.issues}
              total={snapshot.issueCount}
              onOpen={(i) => void api.openUrl(i.url)}
            />
          ) : (
            <WatchingList items={snapshot.watchings} onOpen={(w) => void api.openUrl(w.url)} />
          )}
        </main>
      </div>
    </div>
  );
}
