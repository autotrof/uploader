# Receiver Project Guide

Receiver is a simple portable file manager. Keep the implementation filesystem-first, without a database, and avoid adding abstractions unless they remove real complexity.

## Structure

- `backend/`: Rust Actix Web server, REST API, auth, storage, thumbnails, trash, and embedded frontend.
- `frontend/`: React + Vite UI.
- `docs/`: Markdown documentation for AI agents and maintainers.

## Runtime Configuration

Configuration is loaded from `.env` when present.

- `APP_USERNAME`: login/API username. Defaults to `admin`.
- `APP_PASSWORD`: login/API password. Defaults to `admin`.
- `BIND_ADDRESS`: server address. Defaults to `0.0.0.0:8080`.
- `FILE_ROOT`: managed file root. If unset, Receiver uses `storage/` next to the running binary.

Never commit `.env` or `storage/`.

## Architecture Rules

- Do not add a database.
- All user file paths must pass through path normalization and must never escape `FILE_ROOT`.
- Do not expose `.receiver` in file listing, search, download, or UI explorer views.
- UI login uses an HttpOnly cookie session.
- API clients may use Basic Auth.
- Every UI operation must call the same REST API that external clients can use.

## Storage Layout

Inside `FILE_ROOT`, Receiver stores user files directly. Internal data lives in `FILE_ROOT/.receiver/`.

- `.receiver/settings.json`: trash mode setting.
- `.receiver/thumbnails/`: hidden generated thumbnails.
- `.receiver/uploads/`: hidden temporary chunk upload sessions.
- `.receiver/trash/index.json`: trash metadata.
- `.receiver/trash/items/`: deleted file/folder payloads.

## Upload Rules

- Uploads use chunk sessions.
- Default frontend chunk size is 5 MB.
- Chunks are raw request bodies, not multipart.
- `complete` merges chunks, creates missing target folders, applies optional image resize, then creates a 128 px thumbnail for supported images.
- If `force` is false and the target exists, return `409 Conflict`.

## Trash Rules

- Trash mode is stored in `.receiver/settings.json`.
- When enabled, delete moves the whole file/folder into `.receiver/trash/items/`.
- Trash items are retained for 30 days.
- Cleanup runs on startup and daily.
- Restore should fail with `409 Conflict` if the original path already exists.

## Build

Build frontend first, then backend:

```bash
cd frontend
npm ci
npm run build
cd ../backend
cargo build --release --target x86_64-unknown-linux-musl
```

The final release artifact is the backend binary; frontend assets are embedded.

## Git Rules

- Do not run git commands that modify current git state.
- Read-only git commands are allowed when needed.
- Do not revert user changes unless explicitly requested.
