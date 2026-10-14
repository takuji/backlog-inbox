import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
// waitFor は vitest の vi.waitFor ではなく Testing Library のものを使う。
// あちらはポーリングを act() で包まないため、React の更新が act(...) 警告に
// なりうる。
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { SettingsForm } from "./SettingsForm";
import { I18nProvider } from "../i18n";
import * as api from "../api";

// vitest はデフォルトで global な afterEach を提供しないため、
// @testing-library/react の自動クリーンアップが効かない。明示的に呼ぶ。
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// **表示言語を日本語に固定する。** `I18nProvider` の既定は "auto" で、
// `detectOsLang()` は `navigator.language` を見る。jsdom の既定は "en-US"
// なので、固定しないと英語UIが描かれ、日本語文言のアサーションが
// 「常に一致しない=常に通る」空振りテストになる。
// キーは i18n.tsx の STORAGE_KEY と同じ値。
beforeEach(() => {
  localStorage.setItem("backlog-inbox.lang", "ja");
});

// Tauri の invoke は jsdom では動かない。`I18nProvider` の setLanguage は
// 副作用として呼ばれるだけなので解決済みの vi.fn() に差し替え、
// clearSettings は呼び出しを検証するため差し替える。
// getShowUnreadCount は**このテストでは呼ばれない**(`SettingsForm` は
// `navigator.userAgent` に "Mac" を含むときだけ呼び、jsdom の既定は
// 含まない)。実装が変わって呼ばれるようになったときに未処理の invoke で
// 落ちないよう、保険として差し替えてある。
// 他の api は実物のまま(このテストからは呼ばれない)。
vi.mock("../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../api")>();
  return {
    ...actual,
    clearSettings: vi.fn().mockResolvedValue(undefined),
    setLanguage: vi.fn().mockResolvedValue(undefined),
    getShowUnreadCount: vi.fn().mockResolvedValue(false),
  };
});

const connected: api.Settings = {
  spaceHost: "example.backlog.com",
  apiKey: "secret",
};

function renderForm(props: {
  initial: api.Settings | null;
  onCleared?: () => void;
}) {
  const onCleared = props.onCleared ?? vi.fn();
  const result = render(
    <I18nProvider>
      <SettingsForm
        initial={props.initial}
        onSaved={() => {}}
        onCleared={onCleared}
      />
    </I18nProvider>
  );
  return { ...result, onCleared };
}

describe("SettingsForm の接続解除", () => {
  it("未接続では解除ボタンを出さない", () => {
    renderForm({ initial: null });
    expect(screen.queryByRole("button", { name: "接続を解除" })).toBeNull();
  });

  // インライン2段階の存在意義そのもの。1クリックで消えてはいけない。
  it("1回押しただけでは clearSettings を呼ばない", () => {
    renderForm({ initial: connected });
    fireEvent.click(screen.getByRole("button", { name: "接続を解除" }));

    expect(api.clearSettings).not.toHaveBeenCalled();
    // 確認の文言と2つのボタンが出ている
    expect(screen.getByRole("button", { name: "解除する" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "やめる" })).toBeTruthy();
  });

  it("「解除する」で clearSettings と onCleared を呼ぶ", async () => {
    const { onCleared } = renderForm({ initial: connected });
    fireEvent.click(screen.getByRole("button", { name: "接続を解除" }));
    fireEvent.click(screen.getByRole("button", { name: "解除する" }));

    await waitFor(() => expect(onCleared).toHaveBeenCalledTimes(1));
    expect(api.clearSettings).toHaveBeenCalledTimes(1);
  });

  it("「やめる」で確認を閉じ、clearSettings は呼ばない", () => {
    renderForm({ initial: connected });
    fireEvent.click(screen.getByRole("button", { name: "接続を解除" }));
    fireEvent.click(screen.getByRole("button", { name: "やめる" }));

    expect(api.clearSettings).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "接続を解除" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "解除する" })).toBeNull();
  });

  it("失敗したらエラーを表示し、onCleared は呼ばない", async () => {
    vi.mocked(api.clearSettings).mockRejectedValueOnce("キーチェーンに失敗");
    const { onCleared } = renderForm({ initial: connected });
    fireEvent.click(screen.getByRole("button", { name: "接続を解除" }));
    fireEvent.click(screen.getByRole("button", { name: "解除する" }));

    await waitFor(() =>
      expect(screen.getByText(/キーチェーンに失敗/)).toBeTruthy()
    );
    expect(onCleared).not.toHaveBeenCalled();
  });
});
