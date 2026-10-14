import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import App from "./App";
import { I18nProvider } from "./i18n";
import * as api from "./api";

// vitest はデフォルトで global な afterEach を提供しないため、
// @testing-library/react の自動クリーンアップが効かない。明示的に呼ぶ。
afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

// **表示言語を日本語に固定する。** `I18nProvider` の既定は "auto" で、
// `detectOsLang()` は `navigator.language`(jsdom の既定は "en-US")を見る。
// 固定しないと英語UIが描かれ、日本語文言のアサーションが空振りする。
// キーは i18n.tsx の STORAGE_KEY と同じ値。
beforeEach(() => {
  localStorage.setItem("backlog-inbox.lang", "ja");
});

// Tauri のイベント購読は jsdom では動かない。ハンドラを捕まえておき、
// テストからバックエンド由来のイベントを配れるようにする(`emitSnapshot`)。
const { listeners } = vi.hoisted(() => ({
  listeners: new Map<string, (e: { payload: unknown }) => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, handler: (e: { payload: unknown }) => void) => {
    listeners.set(event, handler);
    return () => listeners.delete(event);
  }),
}));

/// バックエンドが emit する `snapshot` イベントを再現する。
function emitSnapshot(next: api.Snapshot) {
  const handler = listeners.get("snapshot");
  if (!handler) throw new Error("snapshot の購読がまだ張られていない");
  act(() => handler({ payload: next }));
}

// Tauri の invoke は jsdom では動かないので、`App` が起動時とこの操作で
// 呼ぶものだけを差し替える。他の api は実物のまま(このテストからは呼ばれない)。
vi.mock("./api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./api")>();
  return {
    ...actual,
    getSettings: vi.fn(),
    getSnapshot: vi.fn(),
    getSpaceLogo: vi.fn(),
    getMcpStatus: vi.fn(),
    getShowUnreadCount: vi.fn(),
    setLanguage: vi.fn(),
    clearSettings: vi.fn(),
    openUrl: vi.fn(),
    markNotificationRead: vi.fn(),
    markResourceViewed: vi.fn(),
    saveSettings: vi.fn(),
    getUserIcon: vi.fn(),
  };
});

const connected: api.Settings = {
  spaceHost: "example.backlog.com",
  apiKey: "secret",
};

const snapshot: api.Snapshot = {
  spaceName: "example",
  notifications: [],
  hasMoreNotifications: false,
  unreadCount: 0,
  issues: [],
  issueCount: 0,
  watchings: [],
  skippedNotifications: 0,
};

beforeEach(() => {
  vi.mocked(api.getSettings).mockResolvedValue(connected);
  vi.mocked(api.getSnapshot).mockResolvedValue(snapshot);
  vi.mocked(api.getSpaceLogo).mockRejectedValue("no logo");
  vi.mocked(api.getMcpStatus).mockResolvedValue({
    enabled: false,
    running: false,
    url: null,
    error: null,
  });
  vi.mocked(api.getShowUnreadCount).mockResolvedValue(false);
  vi.mocked(api.setLanguage).mockResolvedValue(undefined);
  vi.mocked(api.clearSettings).mockResolvedValue(undefined);
  vi.mocked(api.openUrl).mockResolvedValue(undefined);
  vi.mocked(api.markNotificationRead).mockResolvedValue(undefined);
  vi.mocked(api.markResourceViewed).mockResolvedValue(undefined);
  vi.mocked(api.saveSettings).mockResolvedValue({ id: 1, name: "me" });
  vi.mocked(api.getUserIcon).mockRejectedValue("no icon");
});

/// 既読だがリソースは未閲覧の通知。**この状態がクリックで暗転する対象。**
/// 未読のものは `alreadyRead` が false なので暗転条件を満たさず、通知タブを
/// 見ているだけで自動既読化されるため、一覧に並ぶのは実際にはこの状態。
const readNotification: api.NotificationItem = {
  id: 1,
  alreadyRead: true,
  resourceAlreadyRead: false,
  reason: 2,
  senderId: null,
  senderName: "送信者",
  projectName: "プロジェクト",
  title: "件名",
  commentSnippet: null,
  url: "https://example.test/1",
  created: "2026-07-30T07:55:07Z",
};

/// 送信者・種別・閲覧状態がそれぞれ異なる3件。絞り込みの検証用。
const yamada: api.NotificationItem = {
  ...readNotification,
  id: 1,
  senderId: 10,
  senderName: "山田",
  reason: 2,
  title: "山田のコメント",
};
const sato: api.NotificationItem = {
  ...readNotification,
  id: 2,
  senderId: 20,
  senderName: "佐藤",
  reason: 4,
  title: "佐藤の更新",
};
const viewed: api.NotificationItem = {
  ...readNotification,
  id: 3,
  senderId: 10,
  senderName: "山田",
  reason: 2,
  resourceAlreadyRead: true,
  title: "閲覧済みの件名",
};

/// 通知を積んだ状態で描画し、絞り込みパネルを開く。
/// `rest` で `hasMoreNotifications` などフィクスチャの既定を上書きできる。
async function renderWithFilterOpen(
  items: api.NotificationItem[],
  rest: Partial<api.Snapshot> = {}
) {
  vi.mocked(api.getSnapshot).mockResolvedValue({
    ...snapshot,
    notifications: items,
    ...rest,
  });
  render(
    <I18nProvider>
      <App />
    </I18nProvider>
  );
  fireEvent.click(await screen.findByRole("button", { name: "絞り込み" }));
}

/// 一覧の行(クリック対象のボタン)を件名から引く。
function row(): HTMLElement {
  return screen.getByText("件名").closest("button") as HTMLElement;
}

/// 接続済みで起動し、設定画面を開いて「接続を解除」を確定するところまで進める。
async function disconnectFromSettings() {
  render(
    <I18nProvider>
      <App />
    </I18nProvider>
  );
  fireEvent.click(await screen.findByTitle("設定"));
  fireEvent.click(await screen.findByRole("button", { name: "接続を解除" }));
  fireEvent.click(screen.getByRole("button", { name: "解除する" }));
}

describe("App の接続解除後の画面", () => {
  /// **解除後、入力欄が空になっていること。**
  ///
  /// `SettingsForm` は `initial` をマウント時に一度だけローカル state へ
  /// 写す。`App` が同じ位置に描き続けると React は同一インスタンスとして
  /// 差分更新するため、`initial=null` になっても入力欄には削除済みの
  /// API キーが残ったままになる。そのまま「保存して接続」を押せば、
  /// 消したはずの鍵で黙って再接続してしまう。
  /// `initial` に応じた `key` で作り直させることでしか防げない。
  it("スペースURLとAPIキーの入力欄が空に戻る", async () => {
    await disconnectFromSettings();

    // 未接続画面になったこと(リード文は `initial` が null のときだけ出る)
    await screen.findByText(/スペースのURLとAPIキーを登録すると/);

    await waitFor(() => {
      expect(
        screen.getByPlaceholderText<HTMLInputElement>("example.backlog.com").value
      ).toBe("");
    });
    expect(screen.getByPlaceholderText<HTMLInputElement>("APIキー").value).toBe("");
  });

  /// 解除後は初回起動と同じくスペースURL欄にフォーカスが当たること。
  /// `autoFocus={!initial}` はマウント時にしか効かないので、これも
  /// 作り直しが起きていることの証拠になる。
  it("スペースURL欄にフォーカスが戻る", async () => {
    await disconnectFromSettings();
    await screen.findByText(/スペースのURLとAPIキーを登録すると/);

    await waitFor(() => {
      expect(document.activeElement).toBe(
        screen.getByPlaceholderText("example.backlog.com")
      );
    });
  });
});

describe("App の通知の閲覧済み表示", () => {
  /// **クリックで付けた暗転が、後から届く snapshot で消えないこと。**
  ///
  /// `openNotification` が立てる `resourceAlreadyRead` はフロントの state に
  /// しか無く、バックエンドの `NotifStore` には存在しない(`resource_already_read`
  /// に true を書く箇所がバックエンドには無い)。`snapshot` イベントは通知配列を
  /// 丸ごと差し替えるので、課題タブを開いたときの `fetch_issues` 由来の snapshot
  /// ひとつで印が消え、通知タブに戻ると暗転が外れて見える。
  it("後から届いた snapshot で閲覧済みの暗転が消えない", async () => {
    vi.mocked(api.getSnapshot).mockResolvedValue({
      ...snapshot,
      notifications: [readNotification],
    });
    render(
      <I18nProvider>
        <App />
      </I18nProvider>
    );

    await screen.findByText("件名");
    expect(row().className).not.toContain("dimmed");

    fireEvent.click(row());
    await waitFor(() => expect(row().className).toContain("dimmed"));

    // 課題タブを開いたときにバックエンドが emit する snapshot の再現。
    // 通知は取り直していないので resourceAlreadyRead は false のまま届く。
    emitSnapshot({ ...snapshot, notifications: [readNotification] });

    expect(row().className).toContain("dimmed");
  });

  /// **クリックした事実をバックエンドのストアにも記録すること。**
  ///
  /// サーバーは redirect を開いた時点で resourceAlreadyRead を立てるが、
  /// 更新が取り直すのは1ページ目だけなので、「もっと見る」で遡った通知には
  /// その値が届かない。記録が無いと、暗転しているのに未閲覧の絞り込みに
  /// 残り続けるという食い違った状態になる。
  it("クリックした通知をストアにも閲覧済みとして記録する", async () => {
    vi.mocked(api.getSnapshot).mockResolvedValue({
      ...snapshot,
      notifications: [readNotification],
    });
    render(
      <I18nProvider>
        <App />
      </I18nProvider>
    );

    await screen.findByText("件名");
    fireEvent.click(row());

    await waitFor(() => expect(api.markResourceViewed).toHaveBeenCalledWith(1));
  });

  /// **別スペースに繋ぎ直したら閲覧済みを持ち越さないこと。**
  ///
  /// 通知IDはスペースごとの連番なので、繋ぎ直した先で同じIDの別通知に
  /// 出会う確率は低くない。クリック履歴を持ち越すと、開いてもいない通知が
  /// 閲覧済みとして暗転する。
  it("別スペースへ繋ぎ直すと閲覧済みを持ち越さない", async () => {
    vi.mocked(api.getSnapshot).mockResolvedValue({
      ...snapshot,
      notifications: [readNotification],
    });
    render(
      <I18nProvider>
        <App />
      </I18nProvider>
    );

    await screen.findByText("件名");
    fireEvent.click(row());
    await waitFor(() => expect(row().className).toContain("dimmed"));

    // 解除して別スペースへ接続し直す
    fireEvent.click(await screen.findByTitle("設定"));
    fireEvent.click(await screen.findByRole("button", { name: "接続を解除" }));
    fireEvent.click(screen.getByRole("button", { name: "解除する" }));
    await screen.findByText(/スペースのURLとAPIキーを登録すると/);

    vi.mocked(api.getSettings).mockResolvedValue({
      spaceHost: "another.backlog.com",
      apiKey: "another-key",
    });
    fireEvent.change(screen.getByPlaceholderText("example.backlog.com"), {
      target: { value: "another.backlog.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("APIキー"), {
      target: { value: "another-key" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存して接続" }));

    // 繋ぎ直した先の snapshot。ID は同じだがリソースは未閲覧
    await waitFor(() => expect(api.saveSettings).toHaveBeenCalled());
    emitSnapshot({ ...snapshot, notifications: [readNotification] });

    await screen.findByText("件名");
    expect(row().className).not.toContain("dimmed");
  });
});

describe("App の通知の絞り込み", () => {
  it("送信者で絞ると他の送信者の通知が消える", async () => {
    await renderWithFilterOpen([yamada, sato]);

    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "10" } });

    expect(screen.queryByText("佐藤の更新")).toBeNull();
    expect(screen.getByText("山田のコメント")).toBeTruthy();
  });

  /// **件数表示が App の配線ごと正しいこと。**
  ///
  /// `NotificationFilter` 単体のテストは `total` / `shown` を静的に渡すので、
  /// コンポーネント内部の取り違えしか捕まえられない。呼び出し側で `shown` に
  /// 別の配列の長さを渡す取り違えはここでしか落ちない。この件数表示は
  /// 「絞り込むと『もっと見る』で増えない」に対する設計の中心的な緩和策なので、
  /// 黙って壊れると痛い。
  it("分母は取得済みの件数、分子は合致した件数になる", async () => {
    await renderWithFilterOpen([yamada, sato]);

    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "10" } });

    expect(screen.getByText("取得済み2件中1件")).toBeTruthy();
  });

  it("種別で絞ると他の種別の通知が消える", async () => {
    await renderWithFilterOpen([yamada, sato]);

    fireEvent.change(screen.getByLabelText("種別"), { target: { value: "2" } });

    expect(screen.queryByText("佐藤の更新")).toBeNull();
    expect(screen.getByText("山田のコメント")).toBeTruthy();
  });

  it("未閲覧のみで閲覧済みの通知が消える", async () => {
    await renderWithFilterOpen([yamada, viewed]);

    fireEvent.click(screen.getByLabelText("未閲覧のものだけ"));

    expect(screen.queryByText("閲覧済みの件名")).toBeNull();
    expect(screen.getByText("山田のコメント")).toBeTruthy();
  });

  /// **絞り込み中にクリックしても一覧から消えないこと。**
  ///
  /// `viewedIds` を被せた後の値で絞ると、クリックした瞬間にその通知が消えて
  /// 下の項目が繰り上がる。クリックはブラウザーでページを開く操作なので、
  /// 続けてクリックすると意図しないページを開く事故になる。生の値で絞れば
  /// その場に残って暗くなるだけで済む。
  it("未閲覧のみで絞り込み中にクリックしても消えず、暗転だけする", async () => {
    await renderWithFilterOpen([yamada, sato]);
    fireEvent.click(screen.getByLabelText("未閲覧のものだけ"));

    const target = screen.getByText("山田のコメント").closest("button") as HTMLElement;
    fireEvent.click(target);

    await waitFor(() =>
      expect(
        (screen.getByText("山田のコメント").closest("button") as HTMLElement).className
      ).toContain("dimmed")
    );
    expect(screen.getByText("山田のコメント")).toBeTruthy();
  });

  /// 課題タブを開いたときの取得などで snapshot が届いても条件が保たれること。
  it("snapshot が届いても絞り込みが保たれる", async () => {
    await renderWithFilterOpen([yamada, sato]);
    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "10" } });
    expect(screen.queryByText("佐藤の更新")).toBeNull();

    emitSnapshot({ ...snapshot, notifications: [yamada, sato] });

    expect(screen.queryByText("佐藤の更新")).toBeNull();
    expect(screen.getByText("山田のコメント")).toBeTruthy();
  });

  /// **合致0件でも続きを取得する手段を残すこと。**
  ///
  /// `App` は絞り込み**後**の配列を `items` に、**生の** `hasMore` を渡す。
  /// 0件で早期 return してしまうと、続きを取りに行く唯一のボタンが画面から
  /// 消え、絞り込みを解除しない限り2ページ目に辿り着けなくなる。1件合致なら
  /// ボタンが残り0件だと消えるという非対称は、利用者にはバグに見える。
  /// 分母が動くことで「押しても何も起きなかった」ではないと分かる、という
  /// 設計の緩和策も、このボタンが残っていることが前提になっている。
  it("合致0件でも続きの取得ボタンが残る", async () => {
    await renderWithFilterOpen([viewed], { hasMoreNotifications: true });

    // 閲覧済みしか無い一覧を「未閲覧のみ」で絞る → 合致0件
    fireEvent.click(screen.getByLabelText("未閲覧のものだけ"));

    expect(screen.getByText("条件に合う通知はありません")).toBeTruthy();
    expect(screen.getByRole("button", { name: "さらに取得する" })).toBeTruthy();
  });

  /// **同じ送信者が違う名前で並んでいたら、新しい方の名前を選択肢にすること。**
  ///
  /// 一覧は新しい順なので、素直に `Map.set` で潰すと配列の最後＝最も古い出現の
  /// 名前が残る。改名した相手が旧姓で並び続けることになり、探している名前で
  /// 見つけられない。
  it("送信者名が変わっていたら新しい方を選択肢に出す", async () => {
    await renderWithFilterOpen([
      { ...yamada, id: 2, senderName: "山田(新)" },
      { ...yamada, id: 1, senderName: "山田(旧)" },
    ]);

    expect(screen.getByRole("option", { name: "山田(新)" })).toBeTruthy();
    expect(screen.queryByRole("option", { name: "山田(旧)" })).toBeNull();
  });

  /// **選んだ送信者が取得し直しで消えても、黙って解除しないこと。**
  ///
  /// 自動で解除すると、絞り込んでいるつもりの画面に全件が出る。
  it("選んだ送信者が一覧から消えても絞り込みが解除されない", async () => {
    await renderWithFilterOpen([yamada, sato]);
    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "10" } });

    // 山田の通知が1件も無い snapshot が届く
    emitSnapshot({ ...snapshot, notifications: [sato] });

    expect(screen.queryByText("佐藤の更新")).toBeNull();
    expect(screen.getByText("条件に合う通知はありません")).toBeTruthy();
  });

  /// **消えた選択肢を、パネルが選択中のまま出し続けること。**
  ///
  /// 解除しないだけでは足りない。一致する `<option>` が無い `<select>` は
  /// HTML の仕様上どれも選択されず(`selectedIndex === -1`)、**空欄**で描かれる。
  /// 「条件に合う通知はありません」の隣に空のドロップダウンが並び、利用者は
  /// 何で絞られているのか見えず、選び直すこともできなくなる。
  it("選んだ送信者が一覧から消えてもパネルが選択中のまま表示する", async () => {
    await renderWithFilterOpen([yamada, sato]);
    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "10" } });

    // 山田の通知が1件も無い snapshot が届く
    emitSnapshot({ ...snapshot, notifications: [sato] });

    expect(screen.getByRole("option", { name: "山田" })).toBeTruthy();
    expect(screen.getByLabelText<HTMLSelectElement>("送信者").value).toBe("10");
  });

  /// 種別も同じ。こちらは `t.reason()` が任意のコードを表示できるので
  /// 選択中のコードを選択肢に合流させるだけで済む。
  it("選んだ種別が一覧から消えてもパネルが選択中のまま表示する", async () => {
    await renderWithFilterOpen([yamada, sato]);
    fireEvent.change(screen.getByLabelText("種別"), { target: { value: "4" } });

    // reason 4 の通知が1件も無い snapshot が届く
    emitSnapshot({ ...snapshot, notifications: [yamada] });

    expect(screen.getByLabelText<HTMLSelectElement>("種別").value).toBe("4");
  });

  /// **別スペースへ繋ぎ直したら条件を持ち越さないこと。**
  ///
  /// 送信者IDもスペースごとの連番なので、持ち越すと繋ぎ直した先の別人で
  /// 絞られたままになる。通知が届いているのに条件から外れていて気づかない
  /// という、このアプリで最も起こしてはいけない状態になる。
  it("別スペースへ繋ぎ直すと絞り込みが持ち越されない", async () => {
    await renderWithFilterOpen([yamada, sato]);
    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "10" } });
    expect(screen.queryByText("佐藤の更新")).toBeNull();

    // 解除して別スペースへ接続し直す
    fireEvent.click(await screen.findByTitle("設定"));
    fireEvent.click(await screen.findByRole("button", { name: "接続を解除" }));
    fireEvent.click(screen.getByRole("button", { name: "解除する" }));
    await screen.findByText(/スペースのURLとAPIキーを登録すると/);

    vi.mocked(api.getSettings).mockResolvedValue({
      spaceHost: "another.backlog.com",
      apiKey: "another-key",
    });
    fireEvent.change(screen.getByPlaceholderText("example.backlog.com"), {
      target: { value: "another.backlog.com" },
    });
    fireEvent.change(screen.getByPlaceholderText("APIキー"), {
      target: { value: "another-key" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存して接続" }));
    await waitFor(() => expect(api.saveSettings).toHaveBeenCalled());

    emitSnapshot({ ...snapshot, notifications: [yamada, sato] });

    // 条件が破棄されているので両方見える
    await screen.findByText("佐藤の更新");
    expect(screen.getByText("山田のコメント")).toBeTruthy();
  });
});
