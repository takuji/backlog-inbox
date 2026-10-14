import * as api from "../api";
import { useI18n } from "../i18n";
import { relativeTime } from "../utils";
import { Avatar } from "./Avatar";

interface Props {
  items: api.WatchingItem[];
  onOpen: (item: api.WatchingItem) => void;
}

export function WatchingList({ items, onOpen }: Props) {
  const { t } = useI18n();
  if (items.length === 0) {
    return <div className="empty">{t.watchEmpty}</div>;
  }
  const updatedCount = items.filter((w) => w.updated).length;
  return (
    <div className="list-wrap">
      <div className="list-toolbar">
        <span className="count-line">{t.watchCount(items.length)}</span>
        {updatedCount > 0 && (
          <span className="badge badge-blue">{t.watchUpdatedCount(updatedCount)}</span>
        )}
      </div>
      <ul className="list">
        {items.map((w) => (
          <li key={w.id}>
            <button
              className={`item ${w.updated ? "unread" : ""}`}
              onClick={() => onOpen(w)}
              title={t.openInBrowser}
            >
              <div className="item-head">
                <Avatar userId={w.creatorId} name={w.creatorName} />
                <div className="item-body">
                  <div className="item-title">
                    <span className="issue-key">{w.issueKey}</span>
                    {w.summary}
                    {w.updated && <span className="badge badge-blue">{t.watchUpdatedBadge}</span>}
                  </div>
                  <div className="item-meta">
                    <span className="chip">{w.statusName}</span>
                    {w.lastContentUpdated && (
                      <span>{t.watchUpdatedAt(relativeTime(w.lastContentUpdated, t))}</span>
                    )}
                  </div>
                </div>
              </div>
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
