// package.json のバージョンを Rust 側(Cargo.toml / tauri.conf.json / Cargo.lock)へ同期する。
// `pnpm version x.y.z` の `version` ライフサイクルで実行され、変更をコミットに含める。
import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const { version } = JSON.parse(readFileSync("package.json", "utf8"));

// [package] name = "backlog-inbox" 直後の version(lib 名は underscore なので当たらない)
const packageVersion = /(name = "backlog-inbox"\r?\nversion = ")[^"]*(")/;

const edits = [
  ["src-tauri/Cargo.toml", packageVersion],
  ["src-tauri/Cargo.lock", packageVersion],
  ["src-tauri/tauri.conf.json", /("version":\s*")[^"]*(")/],
];

for (const [path, pattern] of edits) {
  const before = readFileSync(path, "utf8");
  const after = before.replace(pattern, `$1${version}$2`);
  if (after === before) {
    throw new Error(`version pattern not found (or unchanged) in ${path}`);
  }
  writeFileSync(path, after);
}

// version コミットに含める
execSync("git add " + edits.map(([p]) => p).join(" "), { stdio: "inherit" });

console.log(`Synced version ${version} to Cargo.toml, Cargo.lock, tauri.conf.json`);
