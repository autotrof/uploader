# Receiver Overview

Receiver is a very small file manager served as one portable Linux binary. It has a Rust Actix Web backend and a React frontend embedded in the executable.

## Features

- Login from the browser using credentials from `.env`.
- Basic Auth for API clients.
- Browse files and folders.
- Create empty folders.
- Upload large files using resumable chunk sessions.
- Download files and folders. Folder downloads are ZIP archives.
- Delete files and folders.
- Optional trash mode with restore and permanent delete.
- Search files and folders by name.
- Automatic thumbnails for supported image files.
- Optional image resize during upload.

## Non-goals

- No database.
- No multi-user permissions.
- No built-in TLS. Put Receiver behind a reverse proxy if it is exposed publicly.
