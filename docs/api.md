# API Contract

All API endpoints are under `/api`. Browser UI uses cookie auth after login. External clients can use Basic Auth.

Importable API documents:

- Postman: [`docs/postman-collection.json`](postman-collection.json)
- Swagger/OpenAPI: [`docs/openapi.json`](openapi.json)

Importable API docs:

- Postman collection: [`docs/postman-collection.json`](postman-collection.json)
- OpenAPI/Swagger JSON: [`docs/openapi.json`](openapi.json)

## Auth

- `POST /api/login`
  - Body: `{ "username": "...", "password": "..." }`
  - Sets an HttpOnly cookie on success.
- `POST /api/logout`
- `GET /api/me`

## Files

- `GET /api/files?path=folder/path`
  - Lists direct children of a folder.
- `GET /api/files/download?path=folder-or-file`
  - Files are returned directly.
  - Folders are returned as ZIP archives.
- `DELETE /api/files?path=folder-or-file`
  - Deletes permanently or moves to trash depending on settings.
- `PUT /api/files/rename`
  - Body: `{ "path": "folder-or-file", "new_name": "nama-baru.ext" }`
- `POST /api/folders?path=folder/path&force=0|1`
  - Creates an empty folder.
  - If the folder exists and `force=1`, the request succeeds.
- `GET /api/search?q=name&path=optional/base`
  - Searches by file/folder name.

## Uploads

- `POST /api/uploads`
  - Body:
    ```json
    {
      "path": "target/folder",
      "filename": "video.mp4",
      "total_size": 12345,
      "chunk_size": 5242880,
      "force": false,
      "max_width": null,
      "max_height": null,
      "thumbnail_size": 45678,
      "thumbnail_content_type": "image/jpeg"
    }
    ```
  - `thumbnail_size` dan `thumbnail_content_type` opsional, hanya untuk upload video.
- `POST /api/uploads/batch`
  - Body:
    ```json
    {
      "files": [
        {
          "path": "target/folder",
          "filename": "photo-1.jpg",
          "total_size": 12345,
          "chunk_size": 5242880,
          "force": false,
          "max_width": null,
          "max_height": null,
          "thumbnail_size": 45678,
          "thumbnail_content_type": "image/jpeg"
        },
        {
          "path": "target/folder",
          "filename": "photo-2.jpg",
          "total_size": 67890,
          "chunk_size": 5242880,
          "force": false,
          "max_width": 1920,
          "max_height": 1080
        }
      ]
    }
    ```
  - Returns upload sessions in the same order as the submitted files.
  - Untuk upload folder dari UI, setiap file dikirim dengan `path` sesuai folder relatifnya.
- `PUT /api/uploads/{upload_id}/chunks/{index}`
  - Raw chunk bytes.
- `PUT /api/uploads/{upload_id}/thumbnail`
  - Raw image bytes untuk thumbnail video opsional.
  - `Content-Type` harus sama dengan `thumbnail_content_type` saat membuat session upload.
- `GET /api/uploads/{upload_id}`
  - Returns received chunk indexes for resume.
- `POST /api/uploads/{upload_id}/complete`
  - Merges chunks and saves the final file.
- `DELETE /api/uploads/{upload_id}`
  - Cancels and deletes temporary chunks.

## Settings and Trash

- `GET /api/settings`
- `PUT /api/settings`
  - Body: `{ "trash_enabled": true }`
- `GET /api/trash`
- `POST /api/trash/restore`
  - Body: `{ "id": "trash-id" }`
- `DELETE /api/trash?id=trash-id`

## Errors

Error responses use:

```json
{ "error": "Message" }
```

Common statuses:

- `400`: invalid input.
- `401`: missing or invalid auth.
- `404`: item not found.
- `409`: name/path conflict.
