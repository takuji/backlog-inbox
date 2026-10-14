import { FormEvent, useEffect, useState } from "react";
import * as api from "../api";
import { useI18n, LangPref } from "../i18n";
import { apiSettingsUrl } from "../utils";
import { SettingsSection } from "./SettingsSection";
import { McpSection } from "./McpSection";

interface Props {
  initial: api.Settings | null;
  onSaved: (user: api.User) => void;
  onCleared: () => void;
  onCancel?: () => void;
}

// 未読件数のメニューバー表示は macOS 固有の機能なので、その場合だけ設定を出す
const isMac = navigator.userAgent.includes("Mac");

export function SettingsForm({ initial, onSaved, onCleared, onCancel }: Props) {
  const { t, pref, setPref } = useI18n();
  const [spaceHost, setSpaceHost] = useState(initial?.spaceHost ?? "");
  const [apiKey, setApiKey] = useState(initial?.apiKey ?? "");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showUnreadCount, setShowUnreadCount] = useState(false);
  // 解除の確認を表示中か。誤クリックでAPIキーを失わせないための2段階目
  const [confirming, setConfirming] = useState(false);

  useEffect(() => {
    if (isMac) {
      api.getShowUnreadCount().then(setShowUnreadCount).catch(() => {});
    }
  }, []);

  const toggleUnreadCount = (value: boolean) => {
    setShowUnreadCount(value);
    api.setShowUnreadCount(value).catch(() => {});
  };

  const apiKeyPageUrl = apiSettingsUrl(spaceHost);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const user = await api.saveSettings(spaceHost, apiKey);
      onSaved(user);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const disconnect = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.clearSettings();
      setConfirming(false);
      onCleared();
    } catch (err) {
      // 失敗時は接続済みのまま留まる。onCleared は呼ばない
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="settings-screen">
      <div className="settings-card">
        <div className="settings-card-header">
          <h1>{initial ? t.settingsHeadingConnected : t.settingsHeading}</h1>
          {onCancel && (
            <button className="icon-button" onClick={onCancel} disabled={busy} title={t.close}>
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
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          )}
        </div>
        {!initial && <p className="settings-lead">{t.settingsLead}</p>}
        <SettingsSection title={t.sectionConnection}>
          <form onSubmit={submit}>
            <label className="field">
              <span className="field-label">{t.spaceUrlLabel}</span>
              <input
                type="text"
                value={spaceHost}
                onChange={(e) => setSpaceHost(e.target.value)}
                placeholder="example.backlog.com"
                autoFocus={!initial}
                required
              />
            </label>
            <label className="field">
              <span className="field-label">{t.apiKeyLabel}</span>
              <input
                type="password"
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t.apiKeyPlaceholder}
                required
              />
              {apiKeyPageUrl && (
                <button
                  type="button"
                  className="text-button field-link"
                  onClick={() => void api.openUrl(apiKeyPageUrl)}
                  title={t.openInBrowser}
                >
                  {t.openApiPage}
                </button>
              )}
            </label>
            {error && <div className="error-banner">{error}</div>}
            <div className="section-actions">
              <button type="submit" className="button primary" disabled={busy}>
                {busy ? t.saving : t.save}
              </button>
            </div>
          </form>
          {initial && (
            <div className="disconnect">
              {confirming ? (
                <>
                  <p className="disconnect-warning">{t.disconnectWarning}</p>
                  <div className="disconnect-actions">
                    <button
                      type="button"
                      className="button danger"
                      onClick={() => void disconnect()}
                      disabled={busy}
                    >
                      {t.disconnectConfirm}
                    </button>
                    <button
                      type="button"
                      className="button secondary"
                      onClick={() => setConfirming(false)}
                      disabled={busy}
                    >
                      {t.disconnectCancel}
                    </button>
                  </div>
                </>
              ) : (
                <button
                  type="button"
                  className="text-button"
                  onClick={() => setConfirming(true)}
                  disabled={busy}
                >
                  {t.disconnect}
                </button>
              )}
            </div>
          )}
        </SettingsSection>
        <SettingsSection title={t.sectionDisplay}>
          <label className="field">
            <span className="field-label">{t.languageLabel}</span>
            <select value={pref} onChange={(e) => setPref(e.target.value as LangPref)}>
              <option value="auto">{t.langAuto}</option>
              <option value="ja">{t.langJa}</option>
              <option value="en">{t.langEn}</option>
            </select>
          </label>
          {isMac && initial && (
            <label className="field field-check">
              <input
                type="checkbox"
                checked={showUnreadCount}
                onChange={(e) => toggleUnreadCount(e.target.checked)}
              />
              <span>{t.showUnreadCountLabel}</span>
            </label>
          )}
        </SettingsSection>
        {initial && (
          <SettingsSection title={t.mcpSectionLabel}>
            <McpSection />
          </SettingsSection>
        )}
      </div>
    </div>
  );
}
