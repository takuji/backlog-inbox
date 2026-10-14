import { ReactNode } from "react";

interface Props {
  title: string;
  children: ReactNode;
}

// 設定カード内の小見出し付きグループ。表示専用で状態は持たない
export function SettingsSection({ title, children }: Props) {
  return (
    <section className="settings-section">
      <h2>{title}</h2>
      {children}
    </section>
  );
}
