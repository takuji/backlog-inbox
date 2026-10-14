import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  spaceHost: string;
  apiKey: string;
}

export interface User {
  id: number;
  name: string;
}

export interface NotificationItem {
  id: number;
  alreadyRead: boolean;
  resourceAlreadyRead: boolean;
  reason: number;
  senderId: number | null;
  senderName: string;
  projectName: string;
  title: string;
  commentSnippet: string | null;
  url: string;
  created: string;
}

export interface IssueItem {
  id: number;
  issueKey: string;
  summary: string;
  dueDate: string | null;
  statusName: string;
  url: string;
  creatorId: number | null;
  creatorName: string;
}

export interface WatchingItem {
  id: number;
  updated: boolean;
  issueKey: string;
  summary: string;
  statusName: string;
  url: string;
  lastContentUpdated: string | null;
  creatorId: number | null;
  creatorName: string;
}

export interface Snapshot {
  spaceName: string;
  notifications: NotificationItem[];
  hasMoreNotifications: boolean;
  unreadCount: number;
  issues: IssueItem[];
  issueCount: number;
  watchings: WatchingItem[];
  skippedNotifications: number;
}

export const getSettings = () => invoke<Settings | null>("get_settings");

export const saveSettings = (spaceHost: string, apiKey: string) =>
  invoke<User>("save_settings", { spaceHost, apiKey });

export const clearSettings = () => invoke<void>("clear_settings");

export const getSnapshot = () => invoke<Snapshot | null>("get_snapshot");

export const refresh = () => invoke<Snapshot>("refresh");

export const refreshIssues = () => invoke<Snapshot>("refresh_issues");

export const loadOlderNotifications = () =>
  invoke<Snapshot>("load_older_notifications");

export const markNotificationRead = (id: number) =>
  invoke<void>("mark_notification_read", { id });

export const markResourceViewed = (id: number) =>
  invoke<void>("mark_resource_viewed", { id });

export const markAllRead = () => invoke<Snapshot>("mark_all_read");

export const openUrl = (url: string) => invoke<void>("open_url", { url });

export const getLanguage = () => invoke<string>("get_language");

export const setLanguage = (lang: string) => invoke<void>("set_language", { lang });

export const getShowUnreadCount = () =>
  invoke<boolean>("get_show_unread_count");

export const setShowUnreadCount = (value: boolean) =>
  invoke<void>("set_show_unread_count", { value });

export const getUserIcon = (userId: number) =>
  invoke<string>("get_user_icon", { userId });

export const getSpaceLogo = () => invoke<string>("get_space_logo");

export interface McpStatus {
  enabled: boolean;
  running: boolean;
  url: string | null;
  error: string | null;
}

export const getMcpStatus = () => invoke<McpStatus>("get_mcp_status");

export const setMcpEnabled = (enabled: boolean) =>
  invoke<McpStatus>("set_mcp_enabled", { enabled });
