# Frontend and Backend Notes

## Frontend

The React app is a single page app. It calls the REST API with `credentials: include` so the login cookie is sent automatically.

The UI includes:

- file explorer
- upload
- folder creation
- search
- trash
- settings

Do not add programming terms to user-facing labels unless the feature is explicitly for developers.

## Backend

The backend is Actix Web. It serves `/api/*` endpoints and falls back to embedded frontend assets for other paths.

Keep handlers simple and filesystem-oriented. Shared rules:

- authenticate first
- normalize path input
- reject hidden internal paths
- use JSON errors
- avoid reading large upload requests into memory
