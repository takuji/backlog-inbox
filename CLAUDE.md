# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A Tauri v2 desktop app with a React 19 + TypeScript + Vite frontend that keeps Backlog notifications, assigned issues, and watched items in a tray-resident window. Note the directory nesting: this repo lives at `backlog-companion/backlog-companion/` — run all commands from this inner directory.

Package manager is **pnpm** (referenced by `beforeDevCommand`/`beforeBuildCommand` in `src-tauri/tauri.conf.json`).

## Commands

- `pnpm tauri dev` — run the full desktop app (starts Vite on port 1420, then launches the Tauri window with hot reload for both frontend and Rust)
- `pnpm dev` — frontend only in a browser (Tauri `invoke` calls will fail without the Rust backend)
- `pnpm tauri build` — production build and bundle of the desktop app
- `pnpm build` — typecheck (`tsc`) and build the frontend only; the quickest way to check TypeScript errors
- `pnpm test` — frontend tests (Vitest + jsdom + Testing Library); `pnpm exec vitest` for watch mode
- `cargo check` / `cargo build` (from `src-tauri/`) — check/build the Rust side alone
- `cargo test` (from `src-tauri/`) — Rust tests, including `wiremock`-backed tests of the Backlog API client

Test config lives in `vitest.config.ts`, deliberately kept separate from `vite.config.ts` (which carries Tauri-dev-only `server` settings). There is no linter or formatter configured; keep builds warning-free instead.

## Architecture

Standard Tauri two-process model:

- **Frontend** (`src/`): React app served by Vite. Every Rust call goes through `src/api.ts`, which wraps `invoke("command_name", { args })` and owns the TypeScript shape of `Snapshot` and its items; UI lives in `src/components/`, and all user-facing strings go through `src/i18n.tsx` (ja/en, `Messages` type enforces parity).
- **Backend** (`src-tauri/src/`): Rust process. `main.rs` is a thin shim calling `backlog_inbox_lib::run()` in `lib.rs`, where the Tauri builder, the module list, and the polling intervals live. New Rust commands are defined with `#[tauri::command]` in `commands.rs` and must be registered in the `tauri::generate_handler![...]` list in `lib.rs`. The rest: `backlog.rs` (API client + the DTOs serialised to the frontend), `refresh.rs` (fetch engine, `Snapshot` assembly, polling), `state.rs` (`AppState`, `NotifStore`), plus `settings.rs`, `notify.rs`, `tray.rs`, `i18n.rs`, `mcp.rs`.
- **Snapshot flow**: the backend assembles a single `Snapshot` and emits it on the `"snapshot"` event; the frontend renders from it. Adding a field means touching `backlog::Snapshot`, whatever writes it in `refresh.rs`, and the `Snapshot` interface in `src/api.ts` (serde renames to camelCase).
- **Capabilities** (`src-tauri/capabilities/default.json`): Tauri v2 permission system. Any new plugin or API surface the frontend needs must be granted here (currently `core:default` and `opener:default`).
- **Config** (`src-tauri/tauri.conf.json`): window settings, bundle config, and the dev/build command wiring.

Vite is pinned to port 1420 with `strictPort: true` because Tauri expects a fixed dev URL; the dev server ignores changes under `src-tauri/`.
