import * as api from "../api";
import { useI18n } from "../i18n";
import { dueInfo } from "../utils";
import { Avatar } from "./Avatar";

interface Props {
  items: api.IssueItem[];
  total: number;
  onOpen: (item: api.IssueItem) => void;
}

export function IssueList({ items, total, onOpen }: Props) {
  const { t } = useI18n();
  const overdueCount = items.filter((i) => dueInfo(i.dueDate, t)?.level === "overdue").length;
  return (
    <div className="list-wrap">
      <div className="list-toolbar">
        <span className="count-line">{t.issueCount(total)}</span>
        {overdueCount > 0 && <span className="badge badge-red">{t.issueOverdue(overdueCount)}</span>}
      </div>
      {items.length === 0 ? (
        <div className="empty">{t.issueEmpty}</div>
      ) : (
        <>
          <ul className="list">
            {items.map((issue) => {
              const due = dueInfo(issue.dueDate, t);
              return (
                <li key={issue.id}>
                  <button className="item" onClick={() => onOpen(issue)} title={t.openInBrowser}>
                    <div className="item-head">
                      <Avatar userId={issue.creatorId} name={issue.creatorName} />
                      <div className="item-body">
                        <div className="item-title">
                          <span className="issue-key">{issue.issueKey}</span>
                          {issue.summary}
                        </div>
                        <div className="item-meta">
                          <span className="chip">{issue.statusName}</span>
                          {due ? (
                            <span className={`chip due-${due.level}`}>{t.dueLabel(due.label)}</span>
                          ) : (
                            <span className="chip muted">{t.noDueDate}</span>
                          )}
                        </div>
                      </div>
                    </div>
                  </button>
                </li>
              );
            })}
          </ul>
          {total > items.length && (
            <div className="list-footnote">{t.issueFootnote(items.length)}</div>
          )}
        </>
      )}
    </div>
  );
}
