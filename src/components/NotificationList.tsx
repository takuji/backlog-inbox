import * as api from "../api";
import { useI18n } from "../i18n";
import { relativeTime } from "../utils";
import { Avatar } from "./Avatar";

interface Props {
  items: api.NotificationItem[];
  hasMore: boolean;
  loadingMore: boolean;
  skipped: number;
  filterActive: boolean;
  onOpen: (item: api.NotificationItem) => void;
  onLoadMore: () => void;
}

export function NotificationList({
  items,
  hasMore,
  loadingMore,
  skipped,
  filterActive,
  onOpen,
  onLoadMore,
}: Props) {
  const { t } = useI18n();
  const empty = items.length === 0;
  // **0件でも早期 return しない。** `items` は絞り込み後、`hasMore` は生の値
  // なので、ここで打ち切ると 0 件まで絞ったときだけ続きを取りに行く手段が
  // 消える。1件合致なら押せて0件だと押せないという非対称は利用者にはバグに
  // 見えるし、「分母が動くから何も起きなかったわけではないと分かる」という
  // 緩和策自体が、このボタンが残っていることを前提にしている。
  return (
    <div className="list-wrap">
      {empty ? (
        // 「条件に合わない」「そもそも無い」「読めなくて落ちた」は利用者にとって
        // 全く違う意味なので、必ず区別して出す。
        //
        // **絞り込み中でも skipped は隠さない。** 落ちた通知は中身が分からない
        // 以上、条件に合っていたかどうかも分からない。ここを黙らせると
        // スキップ件数の機能が防ごうとした「黙って消える」がそのまま再発する。
        <div className="empty">
          {filterActive
            ? t.notifFilterEmpty
            : skipped > 0
              ? t.notifSkipped(skipped)
              : t.notifEmpty}
          {filterActive && skipped > 0 && (
            <div className="empty-note">{t.notifSkipped(skipped)}</div>
          )}
        </div>
      ) : (
        <ul className="list">
          {items.map((n) => (
            <li key={n.id}>
              <button
                className={`item ${n.alreadyRead ? "" : "unread"} ${
                  n.alreadyRead && n.resourceAlreadyRead ? "dimmed" : ""
                }`}
                onClick={() => onOpen(n)}
                title={t.openInBrowser}
              >
                <div className="item-head">
                  <Avatar userId={n.senderId} name={n.senderName} />
                  <div className="item-body">
                    <div className="item-title">{n.title}</div>
                    <div className="item-sub">{t.notifBy(n.senderName, t.reason(n.reason))}</div>
                    {n.commentSnippet && <div className="item-snippet">{n.commentSnippet}</div>}
                    <div className="item-meta">
                      <span>{n.projectName}</span>
                      <span>{relativeTime(n.created, t)}</span>
                    </div>
                  </div>
                </div>
              </button>
            </li>
          ))}
        </ul>
      )}
      {/* 0件のときはスキップ件数を空状態の側で既に出しているので重ねない */}
      {!empty && skipped > 0 && (
        <div className="list-footnote">{t.notifSkipped(skipped)}</div>
      )}
      {hasMore && (
        <div className="list-more">
          <button className="button secondary" onClick={onLoadMore} disabled={loadingMore}>
            {loadingMore ? t.loadingMore : filterActive ? t.loadMoreFetch : t.loadMore}
          </button>
        </div>
      )}
    </div>
  );
}
