import type { Messages } from "./i18n";

// Backlogスペースが取り得るドメイン
const BACKLOG_DOMAINS = ["backlog.com", "backlog.jp", "backlogtool.com"];

/**
 * 入力を正規化し、有効なBacklogスペースのホスト名なら返す。
 * スキーム(https://)やパスが付いていても取り除く。妥当でなければ null。
 */
export function normalizeBacklogHost(input: string): string | null {
  const host = input
    .trim()
    .toLowerCase()
    .replace(/^https?:\/\//, "")
    .split("/")[0];
  if (!host) return null;
  for (const domain of BACKLOG_DOMAINS) {
    const suffix = `.${domain}`;
    if (host.endsWith(suffix)) {
      const subdomain = host.slice(0, host.length - suffix.length);
      // スペースキーは 1 ラベル(英数字とハイフン、先頭は英数字)
      if (/^[a-z0-9][a-z0-9-]*$/.test(subdomain)) return host;
    }
  }
  return null;
}

/** 有効なBacklogドメインならAPIキー発行ページのURL、そうでなければ null */
export function apiSettingsUrl(input: string): string | null {
  const host = normalizeBacklogHost(input);
  return host ? `https://${host}/EditApiSettings.action` : null;
}

export function relativeTime(iso: string, t: Messages): string {
  if (!iso) return "";
  const time = new Date(iso).getTime();
  if (Number.isNaN(time)) return "";
  const diffMin = Math.floor((Date.now() - time) / 60000);
  if (diffMin < 1) return t.justNow;
  if (diffMin < 60) return t.minutesAgo(diffMin);
  const h = Math.floor(diffMin / 60);
  if (h < 24) return t.hoursAgo(h);
  const d = Math.floor(h / 24);
  if (d < 7) return t.daysAgo(d);
  const dt = new Date(time);
  return `${dt.getFullYear()}/${dt.getMonth() + 1}/${dt.getDate()}`;
}

export type DueLevel = "overdue" | "today" | "soon" | "normal";

export interface DueInfo {
  label: string;
  level: DueLevel;
}

const DUE_SOON_DAYS = 3;

export function dueInfo(dueDate: string | null, t: Messages): DueInfo | null {
  if (!dueDate) return null;
  const [y, m, d] = dueDate.slice(0, 10).split("-").map(Number);
  if (!y || !m || !d) return null;
  const due = new Date(y, m - 1, d);
  const now = new Date();
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const days = Math.round((due.getTime() - today.getTime()) / 86400000);
  const base = `${m}/${d}`;
  if (days < 0) return { label: t.dueOverdue(base, -days), level: "overdue" };
  if (days === 0) return { label: t.dueToday(base), level: "today" };
  if (days <= DUE_SOON_DAYS) return { label: t.dueInDays(base, days), level: "soon" };
  return { label: base, level: "normal" };
}
