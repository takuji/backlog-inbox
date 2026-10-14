# Backlog Inbox

*[English](README.md) | 日本語*

Backlog の通知や課題をデスクトップで受け取るための常駐アプリ(macOS / Windows)。

ユーザーの環境に常駐し、新しい通知が来たら OS の通知で知らせることで、Backlog に戻るきっかけを作ります。「Backlog を導入してもチームに利用が定着しにくい」という課題をやわらげることが狙いです。

## 主な機能

- **通知の監視と OS 通知** — Backlog の通知を 3 分間隔で自動チェックし、新着を OS の通知で知らせる(手動更新も可能)
- **未読がひと目でわかる** — 未読があるとメニューバー/トレイのアイコンで示し、件数を表示(macOS では Dock バッジも)
- **通知一覧** — クリックするとブラウザーで該当ページを開き、既読にする。「もっと見る」で過去の通知も遡れる
- **担当課題** — 自分が担当の未完了課題を件数付きで一覧。締め切りが近い/過ぎたものを色で強調
- **ウォッチ課題** — ウォッチ中の課題を一覧し、更新のあったものを表示
- **常駐と自動起動** — ウィンドウを閉じても終了せず常駐。ログイン時に自動起動(リリースビルド)
- **日本語 / 英語** — 設定で切り替え可能。既定は OS の言語が日本語なら日本語、それ以外は英語
- **安全な認証情報の保存** — Backlog API キーは OS のセキュアストア(macOS キーチェーン / Windows 資格情報マネージャー)に保存

## 使い方

初回起動時に設定画面が開くので、Backlog の **スペース URL**(例: `example.backlog.com`)と **API キー** を登録します。API キーは Backlog の [個人設定] → [API] で発行できます(設定画面から発行ページを直接開くボタンも表示されます)。

## 技術構成

[Tauri v2](https://v2.tauri.app/) 製。フロントエンドは React 19 + TypeScript + Vite、バックエンドは Rust。Backlog のデータ取得には [Backlog API](https://developer.nulab.com/docs/backlog/) を利用しています。

> このリポジトリは `backlog-companion/backlog-companion/` という入れ子構成です。コマンドはすべて内側のディレクトリで実行してください。パッケージマネージャは pnpm です。

## 開発

```bash
pnpm install          # 依存のインストール
pnpm tauri dev        # デスクトップアプリを開発モードで起動(ホットリロード)
pnpm build            # フロントエンドの型チェック + ビルド(TypeScript エラーの確認に最速)
pnpm tauri build      # 配布用ビルド(.app / .dmg / インストーラを生成)
```

- フロントエンドのみをブラウザーで確認する場合は `pnpm dev`(Rust の `invoke` は動きません)
- Rust 側だけの確認は `src-tauri/` で `cargo check` / `cargo build`

## ライセンス

[MIT](LICENSE)
