import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Vite本体の設定(vite.config.ts)は `server` に固定ポートなど
// Tauri開発専用の設定を持つため、テスト実行には巻き込まず
// 別ファイルとして最小限だけ用意する。
export default defineConfig({
  plugins: [react()],
  test: {
    environment: "jsdom",
  },
});
