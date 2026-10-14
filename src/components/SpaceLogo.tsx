import { useEffect, useState } from "react";
import * as api from "../api";

// スペースロゴを取得して表示する。取得できるまで(または失敗時)は何も出さない。
// スペースが変わったら key を変えて再取得させる。
export function SpaceLogo() {
  const [src, setSrc] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .getSpaceLogo()
      .then((uri) => {
        if (alive) setSrc(uri);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, []);

  if (!src) return null;
  return <img className="space-logo" src={src} alt="" />;
}
