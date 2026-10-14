import { createContext, useContext, useEffect, useMemo, useState, ReactNode } from "react";
import * as api from "./api";

export type Lang = "ja" | "en";
export type LangPref = "auto" | "ja" | "en";

const STORAGE_KEY = "backlog-inbox.lang";

function reasonJa(code: number): string {
  switch (code) {
    case 1: return "課題の担当者に設定しました";
    case 2: return "課題にコメントしました";
    case 3: return "課題を追加しました";
    case 4: return "課題を更新しました";
    case 5: return "ファイルを追加しました";
    case 6: return "プロジェクトメンバーに追加しました";
    case 10: return "プルリクエストの担当者に設定しました";
    case 11: return "プルリクエストにコメントしました";
    case 12: return "プルリクエストを追加しました";
    case 13: return "プルリクエストを更新しました";
    case 14: return "ドキュメントにコメントしました";
    case 15: return "ドキュメントのコメントに返信しました";
    case 16: return "課題を一括登録しました";
    case 17: return "ドキュメントであなたをメンションしました";
    case 18: return "課題の一括登録でお知らせ先に指定しました";
    default: return "通知を送りました";
  }
}

function reasonEn(code: number): string {
  switch (code) {
    case 1: return "assigned an issue to you";
    case 2: return "commented on an issue";
    case 3: return "added an issue";
    case 4: return "updated an issue";
    case 5: return "added a file";
    case 6: return "added you to a project";
    case 10: return "assigned a pull request to you";
    case 11: return "commented on a pull request";
    case 12: return "added a pull request";
    case 13: return "updated a pull request";
    case 14: return "commented on a document";
    case 15: return "replied to a document comment";
    case 16: return "created issues in bulk";
    case 17: return "mentioned you in a document";
    case 18: return "added you to a bulk issue creation";
    default: return "sent you a notification";
  }
}

const ja = {
  tabNotifications: "通知",
  tabIssues: "担当課題",
  tabWatchings: "ウォッチ",
  updatedAt: (time: string) => `${time} 更新`,
  refreshTitle: "今すぐ更新",
  settingsTitle: "設定",
  loading: "読み込み中…",
  noData: "データを取得できていません",

  notifEmpty: "通知はありません",
  loadMore: "もっと見る",
  loadMoreFetch: "さらに取得する",
  filterButton: "絞り込み",
  filterSender: "送信者",
  filterReason: "種別",
  filterAll: "すべて",
  filterUnviewedOnly: "未閲覧のものだけ",
  filterCount: (total: number, shown: number) => `取得済み${total}件中${shown}件`,
  filterClear: "解除",
  notifFilterEmpty: "条件に合う通知はありません",
  loadingMore: "読み込み中…",
  notifBy: (sender: string, reason: string) => `${sender}さんが${reason}`,
  reason: reasonJa,
  openInBrowser: "ブラウザーで開く",

  issueCount: (n: number) => `未完了の課題 ${n} 件`,
  issueOverdue: (n: number) => `期限超過 ${n}`,
  issueEmpty: "担当中の未完了課題はありません",
  dueLabel: (label: string) => `期限 ${label}`,
  noDueDate: "期限なし",
  issueFootnote: (shown: number) => `期限日順に${shown}件まで表示しています`,
  notifSkipped: (n: number) =>
    `直近の取得で${n}件の通知を表示できませんでした(形式が想定と異なります)`,

  watchCount: (n: number) => `ウォッチ中 ${n} 件`,
  watchUpdatedCount: (n: number) => `更新あり ${n}`,
  watchUpdatedBadge: "更新あり",
  watchEmpty: "ウォッチ中の課題はありません",
  watchUpdatedAt: (time: string) => `更新 ${time}`,

  settingsHeading: "Backlogに接続",
  settingsHeadingConnected: "設定",
  settingsLead:
    "スペースのURLとAPIキーを登録すると、通知や課題をこのアプリで受け取れるようになります。",
  sectionConnection: "接続",
  sectionDisplay: "表示",
  spaceUrlLabel: "スペースURL",
  apiKeyLabel: "APIキー",
  apiKeyPlaceholder: "APIキー",
  openApiPage: "APIキー発行ページを開く ↗",
  close: "閉じる",
  save: "保存して接続",
  saving: "接続を確認中…",
  disconnect: "接続を解除",
  disconnectWarning:
    "保存されたAPIキーを削除し、未接続状態に戻ります。取得済みの通知や課題も消えます。",
  disconnectConfirm: "解除する",
  disconnectCancel: "やめる",
  languageLabel: "言語",
  langAuto: "自動（OSに合わせる）",
  langJa: "日本語",
  langEn: "English",
  showUnreadCountLabel: "メニューバーに未読件数を表示する",
  mcpSectionLabel: "MCPサーバー",
  mcpToggleLabel: "有効にする",
  mcpHint: "AIツール（Claude Codeなど）からBacklogの課題を読み書きできるようになります。",
  mcpCopyUrl: "コピー",
  mcpCopied: "コピーしました",
  mcpStartError: (message: string) => `起動に失敗しました: ${message}`,

  justNow: "たった今",
  minutesAgo: (n: number) => `${n}分前`,
  hoursAgo: (n: number) => `${n}時間前`,
  daysAgo: (n: number) => `${n}日前`,

  dueToday: (base: string) => `${base}（今日）`,
  dueInDays: (base: string, n: number) => `${base}（あと${n}日）`,
  dueOverdue: (base: string, n: number) => `${base}（${n}日超過）`,
};

export type Messages = typeof ja;

const en: Messages = {
  tabNotifications: "Notifications",
  tabIssues: "Issues",
  tabWatchings: "Watching",
  updatedAt: (time: string) => `Updated ${time}`,
  refreshTitle: "Refresh now",
  settingsTitle: "Settings",
  loading: "Loading…",
  noData: "No data yet",

  notifEmpty: "No notifications",
  loadMore: "Load more",
  loadMoreFetch: "Fetch more",
  filterButton: "Filter",
  filterSender: "Sender",
  filterReason: "Type",
  filterAll: "All",
  filterUnviewedOnly: "Unviewed only",
  filterCount: (total: number, shown: number) => `${shown} of ${total} fetched`,
  filterClear: "Clear",
  notifFilterEmpty: "No notifications match the filter",
  loadingMore: "Loading…",
  notifBy: (sender: string, reason: string) => `${sender} ${reason}`,
  reason: reasonEn,
  openInBrowser: "Open in browser",

  issueCount: (n: number) => `${n} open issue${n === 1 ? "" : "s"}`,
  issueOverdue: (n: number) => `${n} overdue`,
  issueEmpty: "No open issues assigned to you",
  dueLabel: (label: string) => `Due ${label}`,
  noDueDate: "No due date",
  issueFootnote: (shown: number) => `Showing the first ${shown} by due date`,
  notifSkipped: (n: number) =>
    `${n} notification${n === 1 ? "" : "s"} in the latest fetch could not be shown (unexpected format)`,

  watchCount: (n: number) => `${n} watched`,
  watchUpdatedCount: (n: number) => `${n} updated`,
  watchUpdatedBadge: "Updated",
  watchEmpty: "No watched issues",
  watchUpdatedAt: (time: string) => `Updated ${time}`,

  settingsHeading: "Connect to Backlog",
  settingsHeadingConnected: "Settings",
  settingsLead:
    "Register your space URL and API key to receive Backlog notifications and issues in this app.",
  sectionConnection: "Connection",
  sectionDisplay: "Display",
  spaceUrlLabel: "Space URL",
  apiKeyLabel: "API key",
  apiKeyPlaceholder: "API key",
  openApiPage: "Open the API key page ↗",
  close: "Close",
  save: "Save and connect",
  saving: "Connecting…",
  disconnect: "Disconnect",
  disconnectWarning:
    "This deletes the saved API key and returns to the unconnected state. Fetched notifications and issues are discarded too.",
  disconnectConfirm: "Delete and disconnect",
  disconnectCancel: "Cancel",
  languageLabel: "Language",
  langAuto: "Automatic (match OS)",
  langJa: "日本語",
  langEn: "English",
  showUnreadCountLabel: "Show the unread count in the menu bar",
  mcpSectionLabel: "MCP server",
  mcpToggleLabel: "Enable",
  mcpHint: "Lets AI tools such as Claude Code read and write your Backlog issues.",
  mcpCopyUrl: "Copy",
  mcpCopied: "Copied",
  mcpStartError: (message: string) => `Failed to start: ${message}`,

  justNow: "just now",
  minutesAgo: (n: number) => `${n}m ago`,
  hoursAgo: (n: number) => `${n}h ago`,
  daysAgo: (n: number) => `${n}d ago`,

  dueToday: (base: string) => `${base} (today)`,
  dueInDays: (base: string, n: number) => `${base} (in ${n}d)`,
  dueOverdue: (base: string, n: number) => `${base} (${n}d overdue)`,
};

const messages: Record<Lang, Messages> = { ja, en };

function detectOsLang(): Lang {
  return navigator.language.toLowerCase().startsWith("ja") ? "ja" : "en";
}

export function resolveLang(pref: LangPref): Lang {
  return pref === "auto" ? detectOsLang() : pref;
}

function loadPref(): LangPref {
  const v = localStorage.getItem(STORAGE_KEY);
  return v === "ja" || v === "en" || v === "auto" ? v : "auto";
}

interface I18nContextValue {
  lang: Lang;
  pref: LangPref;
  setPref: (pref: LangPref) => void;
  t: Messages;
}

const I18nContext = createContext<I18nContextValue | null>(null);

export function I18nProvider({ children }: { children: ReactNode }) {
  const [pref, setPrefState] = useState<LangPref>(loadPref);
  const lang = resolveLang(pref);

  // バックエンド(通知・トレイ・エラー文言)の言語も同期させる
  useEffect(() => {
    api.setLanguage(lang).catch(() => {});
  }, [lang]);

  const setPref = (next: LangPref) => {
    localStorage.setItem(STORAGE_KEY, next);
    setPrefState(next);
  };

  const value = useMemo<I18nContextValue>(
    () => ({ lang, pref, setPref, t: messages[lang] }),
    [lang, pref]
  );

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext);
  if (!ctx) throw new Error("useI18n must be used within I18nProvider");
  return ctx;
}
