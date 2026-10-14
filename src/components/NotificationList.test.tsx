import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { NotificationList } from "./NotificationList";
import { I18nProvider } from "../i18n";
import type { NotificationItem } from "../api";

// vitest はデフォルトで global な afterEach を提供しないため、
// @testing-library/react の自動クリーンアップが効かない。明示的に呼び、
// 前のテストのDOMが残ってクエリが複数ヒットする事故を防ぐ。
afterEach(() => cleanup());

// **表示言語を日本語に固定する。** `I18nProvider` の既定は "auto" で、
// `detectOsLang()` は `navigator.language` を見る。jsdom の既定は "en-US"
// なので、固定しないと英語UIが描かれ、日本語文言のアサーションが
// 実際には何も検証していない空振りになる。
// キーは i18n.tsx の STORAGE_KEY と同じ値。
beforeEach(() => {
  localStorage.setItem("backlog-inbox.lang", "ja");
});

// `NotificationList` は `useI18n()` を使うため `I18nProvider` で包む必要がある。
// `I18nProvider` は言語切り替えをバックエンドへ同期する副作用
// (`api.setLanguage` = Tauriの `invoke`)を持つが、jsdom環境には
// `window.__TAURI_INTERNALS__` が無いため失敗する。ただしその失敗は
// `.catch(() => {})` で握りつぶされるので、Tauriのモックは不要。

function item(id: number): NotificationItem {
  return {
    id,
    alreadyRead: false,
    resourceAlreadyRead: false,
    reason: 2,
    senderId: null,
    senderName: "送信者",
    projectName: "プロジェクト",
    title: `件名${id}`,
    commentSnippet: null,
    url: `https://example.test/${id}`,
    created: "2026-07-30T07:55:07Z",
  };
}

function renderList(props: {
  items: NotificationItem[];
  skipped: number;
  hasMore?: boolean;
  filterActive?: boolean;
}) {
  return render(
    <I18nProvider>
      <NotificationList
        items={props.items}
        hasMore={props.hasMore ?? false}
        loadingMore={false}
        skipped={props.skipped}
        filterActive={props.filterActive ?? false}
        onOpen={() => {}}
        onLoadMore={() => {}}
      />
    </I18nProvider>
  );
}

describe("NotificationList", () => {
  it("skipped が 0 のときは注記(list-footnote)を出さない", () => {
    const { container } = renderList({ items: [item(1)], skipped: 0 });
    expect(container.querySelector(".list-footnote")).toBeNull();
  });

  it("skipped > 0 のときは注記が出て、件数を含むこと", () => {
    const { container } = renderList({ items: [item(1)], skipped: 2 });
    const footnote = container.querySelector(".list-footnote");
    expect(footnote).not.toBeNull();
    expect(footnote?.textContent).toContain("2");
  });

  it("items が空でも skipped > 0 なら件数を出し、通常の空表示は出さないこと", () => {
    const { container } = renderList({ items: [], skipped: 2 });
    // このアサーションは beforeEach の言語ピンが必須。このアサーションが無かったら
    // 言語ピンを削除しても他のテストは全て通ってしまう。削除してしまわないよう、
    // 言語ピンに依存するアサーションをここに置く。
    expect(container.textContent).toContain("直近の取得で2件");
    expect(container.textContent).not.toContain("通知はありません");
  });

  /// 絞り込みで0件なのか、そもそも通知が無いのかは利用者にとって別の意味なので
  /// 区別して出す。ここを共通の「通知はありません」で済ませると、条件が
  /// 効いていることに気づけない。
  ///
  /// **`textContent` の部分一致では書けない。** ja の「条件に合う通知はありません」は
  /// 「通知はありません」を部分文字列として含むので、`not.toContain` が必ず失敗する。
  /// `queryByText` は既定で要素のテキスト全体との完全一致なので区別できる。
  it("絞り込み中に0件なら専用の文言を出す", () => {
    renderList({ items: [], skipped: 0, filterActive: true });
    expect(screen.getByText("条件に合う通知はありません")).toBeTruthy();
    expect(screen.queryByText("通知はありません")).toBeNull();
  });

  /// **絞り込み中でもスキップ件数を隠さないこと。**
  ///
  /// 0件の理由が「条件に合わない」ではなく「読めなくて落ちた」かもしれない。
  /// 落ちた通知は中身が分からない以上、条件に合っていたかも分からない。
  /// ここを黙らせるとスキップ件数の機能が防ごうとした「黙って消える」が再発する。
  it("絞り込み中で0件かつスキップがあれば両方出す", () => {
    const { container } = renderList({ items: [], skipped: 2, filterActive: true });
    expect(container.textContent).toContain("条件に合う通知はありません");
    expect(container.textContent).toContain("2");
    expect(container.textContent).toContain("表示できませんでした");
  });

  /// 絞り込んでいないときの出し分けは今までどおり。
  it("絞り込みなしで0件かつスキップがあればスキップの文言だけ出す", () => {
    const { container } = renderList({ items: [], skipped: 2, filterActive: false });
    expect(container.textContent).toContain("表示できませんでした");
    expect(container.textContent).not.toContain("条件に合う通知はありません");
  });

  /// **絞り込んでいないときは今までどおり「もっと見る」であること。**
  ///
  /// これが無いと、文言を無条件で `t.loadMoreFetch` に書き換えても全スイートが
  /// 緑のまま通る。下の「絞り込み中は変わる」だけでは差が確かめられない。
  it("絞り込みなしなら続きの取得ボタンは「もっと見る」", () => {
    const { container } = renderList({ items: [item(1)], skipped: 0, hasMore: true });
    expect(container.textContent).toContain("もっと見る");
    expect(container.textContent).not.toContain("さらに取得する");
  });

  /// 絞り込み中は「もっと見る」が増えるとは限らないので、取得を表す文言にする。
  it("絞り込み中は続きの取得ボタンの文言が変わる", () => {
    const { container } = renderList({
      items: [item(1)],
      skipped: 0,
      hasMore: true,
      filterActive: true,
    });
    expect(container.textContent).toContain("さらに取得する");
    expect(container.textContent).not.toContain("もっと見る");
  });
});
