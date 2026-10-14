import { useState } from "react";
import { useI18n } from "../i18n";

/// 通知一覧の絞り込み条件。3軸すべて AND で効く。
///
/// `senderId` / `reason` の `null` は「すべて」を表す。
///
/// `senderName` は絞り込みには使わない。選んだ送信者が取得し直しで一覧から
/// 消えたとき、パネルに「誰で絞っているか」を出し続けるためだけに持つ。
/// 表示用のデータを条件に混ぜるのは純粋ではないが、名前を別の場所に
/// 覚えさせる仕掛けを増やすよりは小さく収まる(`reason` は `t.reason()` が
/// 任意のコードを表示できるので、同じ控えは要らない)。
export interface NotifFilter {
  senderId: number | null;
  senderName: string | null;
  reason: number | null;
  unviewedOnly: boolean;
}

export const emptyFilter: NotifFilter = {
  senderId: null,
  senderName: null,
  reason: null,
  unviewedOnly: false,
};

/// 「絞り込み中」の唯一の判定。ボタンの印も 0 件時の文言の出し分けも
/// ここを見るので、条件が増えたときに直す場所を1つにしておく。
export function isFilterActive(f: NotifFilter): boolean {
  return f.senderId !== null || f.reason !== null || f.unviewedOnly;
}

interface Props {
  senders: { id: number; name: string }[];
  reasons: number[];
  value: NotifFilter;
  /// 取得済みの通知件数。サーバー上の総数ではない
  total: number;
  /// 条件に合致した件数
  shown: number;
  onChange: (next: NotifFilter) => void;
  onClear: () => void;
}

// 絞り込みパネル。表示専用で、開閉状態だけ自分で持つ。
export function NotificationFilter({
  senders,
  reasons,
  value,
  total,
  shown,
  onChange,
  onClear,
}: Props) {
  const { t } = useI18n();
  const [open, setOpen] = useState(false);
  const active = isFilterActive(value);

  return (
    <>
      <div className="filter-bar">
        <button
          type="button"
          className={`filter-toggle ${active ? "active" : ""}`}
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
        >
          {t.filterButton}
        </button>
      </div>
      {open && (
        <div className="filter-panel">
          <label className="filter-row">
            <span>{t.filterSender}</span>
            <select
              value={value.senderId ?? ""}
              onChange={(e) => {
                const id = e.target.value === "" ? null : Number(e.target.value);
                onChange({
                  ...value,
                  senderId: id,
                  // 選んだ時点でしか名前は手に入らない。あとで一覧から
                  // 消えても表示を続けられるよう、ここで控えておく。
                  senderName:
                    id === null ? null : (senders.find((s) => s.id === id)?.name ?? null),
                });
              }}
            >
              <option value="">{t.filterAll}</option>
              {senders.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.name}
                </option>
              ))}
            </select>
          </label>
          <label className="filter-row">
            <span>{t.filterReason}</span>
            <select
              value={value.reason ?? ""}
              onChange={(e) =>
                onChange({
                  ...value,
                  reason: e.target.value === "" ? null : Number(e.target.value),
                })
              }
            >
              <option value="">{t.filterAll}</option>
              {reasons.map((r) => (
                <option key={r} value={r}>
                  {t.reason(r)}
                </option>
              ))}
            </select>
          </label>
          <label className="filter-check">
            <input
              type="checkbox"
              checked={value.unviewedOnly}
              onChange={(e) => onChange({ ...value, unviewedOnly: e.target.checked })}
            />
            <span>{t.filterUnviewedOnly}</span>
          </label>
          <div className="filter-foot">
            <span>{t.filterCount(total, shown)}</span>
            {active && (
              <button type="button" className="text-button" onClick={onClear}>
                {t.filterClear}
              </button>
            )}
          </div>
        </div>
      )}
    </>
  );
}
