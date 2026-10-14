import { useEffect, useState } from "react";
import * as api from "../api";
import { useI18n } from "../i18n";

// 接続済みのときだけ親が描画する。MCP の状態は自分で取得する
export function McpSection() {
  const { t } = useI18n();
  const [status, setStatus] = useState<api.McpStatus | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    api.getMcpStatus().then(setStatus).catch(() => {});
  }, []);

  const toggle = async (value: boolean) => {
    try {
      setStatus(await api.setMcpEnabled(value));
    } catch {
      // 失敗時は現状を取り直して表示を実態に合わせる
      api.getMcpStatus().then(setStatus).catch(() => {});
    }
  };

  const copyUrl = async () => {
    if (!status?.url) return;
    try {
      await navigator.clipboard.writeText(status.url);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // クリップボードが使えない環境では何もしない
    }
  };

  return (
    <div className="field">
      <label className="field-check">
        <input
          type="checkbox"
          checked={status?.enabled ?? false}
          disabled={!status}
          onChange={(e) => void toggle(e.target.checked)}
        />
        <span>{t.mcpToggleLabel}</span>
      </label>
      <p className="field-hint">{t.mcpHint}</p>
      {status?.running && status.url && (
        <div className="mcp-url-row">
          <code>{status.url}</code>
          <button type="button" className="text-button" onClick={() => void copyUrl()}>
            {copied ? t.mcpCopied : t.mcpCopyUrl}
          </button>
        </div>
      )}
      {status?.error && <div className="error-banner">{t.mcpStartError(status.error)}</div>}
    </div>
  );
}
