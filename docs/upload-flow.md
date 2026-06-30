# Chunk Upload Flow

Receiver uses session-based chunk uploads so clients can upload large files through gateways with request-size limits.

1. Client calls `POST /api/uploads` with metadata.
2. Server creates a hidden session under `.receiver/uploads/{upload_id}`.
3. Client sends each chunk with `PUT /api/uploads/{upload_id}/chunks/{index}`.
4. If upload is interrupted, client calls `GET /api/uploads/{upload_id}` and skips chunks already received.
5. Client calls `POST /api/uploads/{upload_id}/complete`.
6. Server validates chunk completeness, merges chunks, applies optional image resize, creates a thumbnail, and deletes temporary chunks.

The frontend default chunk size is 5 MB.
