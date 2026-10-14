import { useEffect, useState } from "react";
import * as api from "../api";

// ユーザーアイコンの data URI キャッシュ。取得済み・取得中を userId で共有し、
// 同じユーザーの重複リクエストを防ぐ。
const iconCache = new Map<number, string>();
const iconInFlight = new Map<number, Promise<string>>();

function loadUserIcon(userId: number): Promise<string> {
  const cached = iconCache.get(userId);
  if (cached) return Promise.resolve(cached);
  let p = iconInFlight.get(userId);
  if (!p) {
    p = api
      .getUserIcon(userId)
      .then((uri) => {
        iconCache.set(userId, uri);
        iconInFlight.delete(userId);
        return uri;
      })
      .catch((e) => {
        iconInFlight.delete(userId);
        throw e;
      });
    iconInFlight.set(userId, p);
  }
  return p;
}

export function Avatar({ userId, name }: { userId: number | null; name: string }) {
  const [src, setSrc] = useState<string | null>(
    userId != null ? iconCache.get(userId) ?? null : null
  );

  useEffect(() => {
    if (userId == null) return;
    const cached = iconCache.get(userId);
    if (cached) {
      setSrc(cached);
      return;
    }
    let alive = true;
    loadUserIcon(userId)
      .then((uri) => {
        if (alive) setSrc(uri);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [userId]);

  if (src) {
    return <img className="avatar" src={src} alt="" />;
  }
  const initial = name.trim().charAt(0).toUpperCase() || "?";
  return <div className="avatar avatar-fallback">{initial}</div>;
}
