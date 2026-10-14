#!/usr/bin/env bash
# .env.local(gitignore 済み)から署名・公証用の環境変数を読み込んで tauri build を実行する。
#   pnpm tauri:build
#   pnpm tauri:build --target universal-apple-darwin
set -euo pipefail
cd "$(dirname "$0")/.."

if [ -f .env.local ]; then
  set -a          # 以降に読み込む変数を自動で export する
  . ./.env.local
  set +a
fi

exec pnpm tauri build "$@"
