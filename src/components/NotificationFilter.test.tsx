import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { NotificationFilter, emptyFilter, isFilterActive } from "./NotificationFilter";
import { I18nProvider } from "../i18n";

afterEach(() => cleanup());

// **表示言語を日本語に固定する。** 既定は "auto" で `navigator.language`
// (jsdom は "en-US")を見るため、固定しないと英語UIが描かれ日本語文言の
// アサーションが空振りする。キーは i18n.tsx の STORAGE_KEY と同じ値。
beforeEach(() => {
  localStorage.setItem("backlog-inbox.lang", "ja");
});

const senders = [
  { id: 1, name: "山田" },
  { id: 2, name: "佐藤" },
];

function renderFilter(props: {
  value?: typeof emptyFilter;
  onChange?: (next: typeof emptyFilter) => void;
  onClear?: () => void;
}) {
  const onChange = props.onChange ?? vi.fn();
  const onClear = props.onClear ?? vi.fn();
  const result = render(
    <I18nProvider>
      <NotificationFilter
        senders={senders}
        reasons={[2, 4]}
        value={props.value ?? emptyFilter}
        total={150}
        shown={5}
        onChange={onChange}
        onClear={onClear}
      />
    </I18nProvider>
  );
  return { ...result, onChange, onClear };
}

describe("isFilterActive", () => {
  it("既定値では絞り込んでいないと判定する", () => {
    expect(isFilterActive(emptyFilter)).toBe(false);
  });

  it("いずれか1軸でも既定でなければ絞り込み中と判定する", () => {
    expect(isFilterActive({ ...emptyFilter, senderId: 1 })).toBe(true);
    expect(isFilterActive({ ...emptyFilter, reason: 2 })).toBe(true);
    expect(isFilterActive({ ...emptyFilter, unviewedOnly: true })).toBe(true);
  });
});

describe("NotificationFilter", () => {
  it("既定では畳まれていて、押すと展開する", () => {
    renderFilter({});
    expect(screen.queryByText("送信者")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "絞り込み" }));

    expect(screen.getByText("送信者")).toBeTruthy();
    expect(screen.getByText("取得済み150件中5件")).toBeTruthy();
  });

  /// 畳んでいても絞り込み中だと分かること。分からないと、通知が来ていないのか
  /// 条件で隠れているのかを取り違える。
  it("絞り込み中はボタンに印が付く", () => {
    const { container } = renderFilter({ value: { ...emptyFilter, unviewedOnly: true } });
    expect(container.querySelector(".filter-toggle.active")).not.toBeNull();
  });

  it("送信者を選ぶと onChange に渡る", () => {
    const { onChange } = renderFilter({});
    fireEvent.click(screen.getByRole("button", { name: "絞り込み" }));

    fireEvent.change(screen.getByLabelText("送信者"), { target: { value: "2" } });

    // 名前も一緒に控える。一覧から消えたあともパネルに出し続けるため。
    expect(onChange).toHaveBeenCalledWith({
      ...emptyFilter,
      senderId: 2,
      senderName: "佐藤",
    });
  });

  it("解除は絞り込み中だけ出る", () => {
    renderFilter({});
    fireEvent.click(screen.getByRole("button", { name: "絞り込み" }));
    expect(screen.queryByRole("button", { name: "解除" })).toBeNull();

    cleanup();
    const { onClear } = renderFilter({ value: { ...emptyFilter, senderId: 1 } });
    fireEvent.click(screen.getByRole("button", { name: "絞り込み" }));
    fireEvent.click(screen.getByRole("button", { name: "解除" }));

    expect(onClear).toHaveBeenCalledTimes(1);
  });
});
