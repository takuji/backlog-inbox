# Backlog Inbox

*English | [日本語](README.ja.md)*

A menu-bar/tray companion app (macOS / Windows) for receiving Backlog notifications and issues on your desktop.

It lives in your environment and surfaces new notifications through the OS notification center, nudging you back to Backlog. The goal is to ease a common problem: even after a team adopts Backlog, day-to-day usage is hard to make stick.

## Features

- **Notification watching and OS notifications** — polls Backlog every 3 minutes and alerts you to new notifications via the OS (manual refresh is also available)
- **Unread at a glance** — the menu-bar/tray icon indicates unread notifications and shows the count (plus a Dock badge on macOS)
- **Notification list** — click to open the relevant page in your browser and mark it read; "Load more" pages back through older notifications
- **Assigned issues** — lists your open assigned issues with a count, highlighting ones that are due soon or overdue
- **Watched issues** — lists the issues you watch and flags the ones with updates
- **Resident with autostart** — closing the window keeps it running in the background; it launches at login (release builds)
- **Japanese / English** — switchable in settings; defaults to Japanese when the OS language is Japanese, otherwise English
- **Secure credential storage** — your Backlog API key is stored in the OS secure store (macOS Keychain / Windows Credential Manager)

## Getting started

On first launch a settings screen opens. Enter your Backlog **space URL** (e.g. `example.backlog.com`) and **API key**. You can issue an API key from Backlog under [Personal Settings] → [API] (the settings screen also offers a button that opens the API key page directly).

## Tech stack

Built with [Tauri v2](https://v2.tauri.app/): a React 19 + TypeScript + Vite frontend and a Rust backend. Backlog data is fetched via the [Backlog API](https://developer.nulab.com/docs/backlog/).

> This repository is nested as `backlog-companion/backlog-companion/`. Run all commands from the inner directory. The package manager is pnpm.

## Development

```bash
pnpm install          # install dependencies
pnpm tauri dev        # run the desktop app in dev mode (hot reload)
pnpm build            # typecheck + build the frontend (fastest way to check TypeScript errors)
pnpm tauri build      # production build (.app / .dmg / installers)
```

- To preview the frontend alone in a browser, use `pnpm dev` (Rust `invoke` calls will not work).
- To check the Rust side only, run `cargo check` / `cargo build` in `src-tauri/`.

## License

[MIT](LICENSE)
