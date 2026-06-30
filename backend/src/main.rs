use actix_web::cookie::{time::Duration as CookieDuration, Cookie};
use actix_web::http::header;
use actix_web::{
    delete, get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use chrono::{DateTime, Duration, Local, Utc};
use futures_util::StreamExt;
use image::ImageFormat;
use log::{Level, LevelFilter};
use rand::{distributions::Alphanumeric, Rng};
use rust_embed::RustEmbed;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::env;
use std::fs::{self, File, OpenOptions};
use std::fmt;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::io::AsyncWriteExt;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;

const INTERNAL_DIR: &str = ".receiver";
const THUMBNAIL_MAX: u32 = 128;
const TRASH_RETENTION_DAYS: i64 = 30;
static FILE_LOGGER: OnceLock<FileLogger> = OnceLock::new();

macro_rules! receiver_debug {
    ($($arg:tt)*) => {
        write_log_line(Level::Debug, module_path!(), format_args!($($arg)*))
    };
}

macro_rules! receiver_info {
    ($($arg:tt)*) => {
        write_log_line(Level::Info, module_path!(), format_args!($($arg)*))
    };
}

macro_rules! receiver_warn {
    ($($arg:tt)*) => {
        write_log_line(Level::Warn, module_path!(), format_args!($($arg)*))
    };
}

macro_rules! receiver_error {
    ($($arg:tt)*) => {
        write_log_line(Level::Error, module_path!(), format_args!($($arg)*))
    };
}

#[derive(RustEmbed)]
#[folder = "../frontend/dist"]
struct Frontend;

#[derive(Clone)]
struct AppState {
    username: String,
    password: String,
    storage_root: PathBuf,
    internal_root: PathBuf,
    sessions: Arc<Mutex<HashSet<String>>>,
}

struct FileLogger {
    level: LevelFilter,
    log_dir: PathBuf,
    state: Mutex<LoggerState>,
}

struct LoggerState {
    current_date: String,
    file: Option<File>,
}

#[derive(Serialize)]
struct ApiError {
    error: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Settings {
    trash_enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize)]
struct LoginResponse {
    ok: bool,
}

#[derive(Deserialize)]
struct PathQuery {
    path: Option<String>,
}

#[derive(Deserialize)]
struct FolderQuery {
    path: Option<String>,
    force: Option<u8>,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    path: Option<String>,
}

#[derive(Serialize)]
struct FileItem {
    name: String,
    path: String,
    kind: String,
    size: u64,
    modified: Option<DateTime<Utc>>,
    thumbnail: Option<String>,
}

#[derive(Deserialize, Serialize, Clone)]
struct CreateUploadRequest {
    path: Option<String>,
    filename: String,
    total_size: u64,
    chunk_size: u64,
    force: Option<bool>,
    max_width: Option<u32>,
    max_height: Option<u32>,
}

#[derive(Serialize)]
struct CreateUploadResponse {
    upload_id: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct UploadSession {
    upload_id: String,
    path: String,
    filename: String,
    total_size: u64,
    chunk_size: u64,
    force: bool,
    max_width: Option<u32>,
    max_height: Option<u32>,
    created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct UploadStatus {
    upload_id: String,
    received_chunks: Vec<u64>,
    complete: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct TrashEntry {
    id: String,
    original_path: String,
    trash_path: String,
    deleted_at: DateTime<Utc>,
    kind: String,
}

#[derive(Deserialize)]
struct RestoreRequest {
    id: String,
}

#[derive(Deserialize)]
struct DeleteTrashQuery {
    id: String,
}

#[derive(Deserialize)]
struct SettingsRequest {
    trash_enabled: bool,
}

fn json_error(status: actix_web::http::StatusCode, message: impl Into<String>) -> HttpResponse {
    HttpResponse::build(status).json(ApiError {
        error: message.into(),
    })
}

fn unauthorized() -> HttpResponse {
    HttpResponse::Unauthorized()
        .insert_header((header::WWW_AUTHENTICATE, "Basic realm=\"receiver\""))
        .json(ApiError {
            error: "Unauthorized".to_string(),
        })
}

fn normalize_relative(input: Option<&str>) -> Result<PathBuf, String> {
    let mut clean = PathBuf::new();
    let raw = input.unwrap_or("").trim();
    if raw.starts_with('/') || raw.starts_with('\\') {
        return Err("Path is not allowed".to_string());
    }
    let raw = raw.trim_matches('/');
    if raw.is_empty() {
        return Ok(clean);
    }
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(part) => {
                let value = part
                    .to_str()
                    .ok_or_else(|| "Path contains invalid text".to_string())?;
                if value == INTERNAL_DIR || value.is_empty() {
                    return Err("Path is not allowed".to_string());
                }
                clean.push(value);
            }
            _ => return Err("Path is not allowed".to_string()),
        }
    }
    Ok(clean)
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if value == "." {
        String::new()
    } else {
        value
    }
}

fn storage_path(state: &AppState, input: Option<&str>) -> Result<PathBuf, String> {
    Ok(state.storage_root.join(normalize_relative(input)?))
}

fn ensure_auth(req: &HttpRequest, state: &AppState) -> Result<(), HttpResponse> {
    if let Some(value) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(value) = value.to_str() {
            if let Some(encoded) = value.strip_prefix("Basic ") {
                if let Ok(decoded) = BASE64.decode(encoded) {
                    if let Ok(decoded) = String::from_utf8(decoded) {
                        if let Some((user, pass)) = decoded.split_once(':') {
                            if user == state.username && pass == state.password {
                                return Ok(());
                            }
                        }
                    }
                }
            }
        }
    }
    if let Some(cookie) = req.cookie("receiver_session") {
        let sessions = state.sessions.lock().map_err(|_| {
            json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Session store is unavailable",
            )
        })?;
        if sessions.contains(cookie.value()) {
            return Ok(());
        }
    }
    receiver_warn!("Authentication failed");
    Err(unauthorized())
}

fn settings_path(state: &AppState) -> PathBuf {
    state.internal_root.join("settings.json")
}

fn trash_index_path(state: &AppState) -> PathBuf {
    state.internal_root.join("trash").join("index.json")
}

fn uploads_root(state: &AppState) -> PathBuf {
    state.internal_root.join("uploads")
}

fn thumbnails_root(state: &AppState) -> PathBuf {
    state.internal_root.join("thumbnails")
}

fn read_settings(state: &AppState) -> Settings {
    fs::read_to_string(settings_path(state))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Settings {
            trash_enabled: false,
        })
}

fn write_settings(state: &AppState, settings: &Settings) -> std::io::Result<()> {
    fs::create_dir_all(&state.internal_root)?;
    fs::write(settings_path(state), serde_json::to_vec_pretty(settings)?)?;
    Ok(())
}

fn read_trash_index(state: &AppState) -> Vec<TrashEntry> {
    fs::read_to_string(trash_index_path(state))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_trash_index(state: &AppState, entries: &[TrashEntry]) -> std::io::Result<()> {
    let path = trash_index_path(state);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(entries)?)?;
    Ok(())
}

fn metadata_modified(path: &Path) -> Option<DateTime<Utc>> {
    path.metadata()
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .map(DateTime::<Utc>::from)
}

fn thumbnail_name(relative: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(display_path(relative).as_bytes());
    format!("{:x}.jpg", hasher.finalize())
}

fn thumbnail_url(relative: &Path, full_path: &Path, state: &AppState) -> Option<String> {
    if !is_supported_image(full_path) {
        return None;
    }
    let name = thumbnail_name(relative);
    let thumb = thumbnails_root(state).join(&name);
    if thumb.exists() {
        Some(format!("/api/thumbnails/{}", name))
    } else {
        None
    }
}

fn file_item(path: &Path, relative: &Path, state: &AppState) -> std::io::Result<FileItem> {
    let metadata = path.metadata()?;
    let kind = if metadata.is_dir() { "folder" } else { "file" }.to_string();
    Ok(FileItem {
        name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string(),
        path: display_path(relative),
        kind,
        size: if metadata.is_file() {
            metadata.len()
        } else {
            0
        },
        modified: metadata_modified(path),
        thumbnail: thumbnail_url(relative, path, state),
    })
}

fn is_supported_image(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp"
            )
        })
        .unwrap_or(false)
}

fn image_format(path: &Path) -> Option<ImageFormat> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "gif" => Some(ImageFormat::Gif),
        "webp" => Some(ImageFormat::WebP),
        "bmp" => Some(ImageFormat::Bmp),
        _ => None,
    }
}

fn process_image(
    path: &Path,
    relative: &Path,
    state: &AppState,
    max_width: Option<u32>,
    max_height: Option<u32>,
) {
    if !is_supported_image(path) {
        return;
    }
    let Ok(mut image) = image::open(path) else {
        receiver_warn!("Failed to open image for processing: {}", display_path(path));
        return;
    };
    if let (Some(width), Some(height)) = (max_width, max_height) {
        if width > 0 && height > 0 && (image.width() > width || image.height() > height) {
            receiver_info!("Resizing image: {} to {}x{}", display_path(path), width, height);
            image = image.resize(width, height, image::imageops::FilterType::Lanczos3);
            if let Some(format) = image_format(path) {
                let _ = image.save_with_format(path, format);
            }
        }
    }
    let thumb = image.thumbnail(THUMBNAIL_MAX, THUMBNAIL_MAX);
    let _ = fs::create_dir_all(thumbnails_root(state));
    let _ = thumb.save_with_format(
        thumbnails_root(state).join(thumbnail_name(relative)),
        ImageFormat::Jpeg,
    );
    receiver_debug!("Generated thumbnail for: {}", display_path(relative));
}

fn session_dir(state: &AppState, upload_id: &str) -> PathBuf {
    uploads_root(state).join(upload_id)
}

fn session_meta_path(state: &AppState, upload_id: &str) -> PathBuf {
    session_dir(state, upload_id).join("session.json")
}

fn read_upload_session(state: &AppState, upload_id: &str) -> Result<UploadSession, HttpResponse> {
    let path = session_meta_path(state, upload_id);
    let text = fs::read_to_string(path).map_err(|_| {
        json_error(
            actix_web::http::StatusCode::NOT_FOUND,
            "Upload session was not found",
        )
    })?;
    serde_json::from_str(&text).map_err(|_| {
        json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Upload session is invalid",
        )
    })
}

fn received_chunks(state: &AppState, upload_id: &str) -> Vec<u64> {
    let chunks = session_dir(state, upload_id).join("chunks");
    let mut indexes: Vec<u64> = fs::read_dir(chunks)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u64>().ok())
        })
        .collect();
    indexes.sort_unstable();
    indexes
}

fn expected_chunk_count(session: &UploadSession) -> u64 {
    if session.total_size == 0 {
        0
    } else {
        (session.total_size + session.chunk_size - 1) / session.chunk_size
    }
}

fn cleanup_trash(state: AppState) {
    receiver_info!("Running trash cleanup");
    let cutoff = Utc::now() - Duration::days(TRASH_RETENTION_DAYS);
    let mut entries = read_trash_index(&state);
    let mut kept = Vec::new();
    for entry in entries.drain(..) {
        if entry.deleted_at < cutoff {
            receiver_info!("Removing expired trash item: {}", entry.id);
            let full_path = state.storage_root.join(&entry.trash_path);
            let _ = if full_path.is_dir() {
                fs::remove_dir_all(full_path)
            } else {
                fs::remove_file(full_path)
            };
        } else {
            kept.push(entry);
        }
    }
    let _ = write_trash_index(&state, &kept);
}

#[post("/api/login")]
async fn login(state: web::Data<AppState>, payload: web::Json<LoginRequest>) -> impl Responder {
    receiver_info!("Login attempt for user: {}", payload.username);
    if payload.username != state.username || payload.password != state.password {
        receiver_warn!("Login failed for user: {}", payload.username);
        return unauthorized();
    }
    let token: String = rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    match state.sessions.lock() {
        Ok(mut sessions) => {
            sessions.insert(token.clone());
        }
        Err(_) => {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Session store is unavailable",
            );
        }
    }
    HttpResponse::Ok()
        .cookie(
            Cookie::build("receiver_session", token)
                .path("/")
                .http_only(true)
                .same_site(actix_web::cookie::SameSite::Lax)
                .finish(),
        )
        .json(LoginResponse { ok: true })
}

#[post("/api/logout")]
async fn logout(req: HttpRequest, state: web::Data<AppState>) -> impl Responder {
    if let Some(cookie) = req.cookie("receiver_session") {
        if let Ok(mut sessions) = state.sessions.lock() {
            sessions.remove(cookie.value());
        }
        receiver_info!("User logged out");
    }
    HttpResponse::Ok()
        .cookie(
            Cookie::build("receiver_session", "")
                .path("/")
                .http_only(true)
                .max_age(CookieDuration::seconds(0))
                .finish(),
        )
        .json(LoginResponse { ok: true })
}

#[get("/api/me")]
async fn me(req: HttpRequest, state: web::Data<AppState>) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    receiver_debug!("User info requested: {}", state.username);
    HttpResponse::Ok().json(serde_json::json!({ "username": state.username }))
}

#[get("/api/files")]
async fn list_files(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PathQuery>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let relative = match normalize_relative(query.path.as_deref()) {
        Ok(path) => path,
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let full_path = state.storage_root.join(&relative);
    if !full_path.exists() {
        return json_error(
            actix_web::http::StatusCode::NOT_FOUND,
            "Folder was not found",
        );
    }
    if !full_path.is_dir() {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Path is not a folder",
        );
    }
    receiver_info!("Listing files in: {}", display_path(&relative));
    let mut items = Vec::new();
    match fs::read_dir(full_path) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if entry.file_name() == INTERNAL_DIR {
                    continue;
                }
                let child_relative = relative.join(entry.file_name());
                if let Ok(item) = file_item(&entry.path(), &child_relative, &state) {
                    items.push(item);
                }
            }
        }
        Err(_) => {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to read folder",
            );
        }
    }
    items.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    HttpResponse::Ok().json(serde_json::json!({ "path": display_path(&relative), "items": items }))
}

#[post("/api/folders")]
async fn create_folder(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<FolderQuery>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let relative = match normalize_relative(query.path.as_deref()) {
        Ok(path) if !path.as_os_str().is_empty() => path,
        Ok(_) => {
            return json_error(
                actix_web::http::StatusCode::BAD_REQUEST,
                "Folder path is required",
            )
        }
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let full_path = state.storage_root.join(&relative);
    receiver_info!("Creating folder: {}", display_path(&relative));
    if full_path.exists() {
        if query.force == Some(1) && full_path.is_dir() {
            return HttpResponse::Ok().json(file_item(&full_path, &relative, &state).ok());
        }
        return json_error(actix_web::http::StatusCode::CONFLICT, "Name already exists");
    }
    match fs::create_dir_all(&full_path) {
        Ok(_) => HttpResponse::Created().json(file_item(&full_path, &relative, &state).ok()),
        Err(_) => json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create folder",
        ),
    }
}

#[get("/api/search")]
async fn search_files(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<SearchQuery>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    receiver_info!("Searching for '{}' in {}", query.q, display_path(&normalize_relative(query.path.as_deref()).unwrap_or_default()));
    let base_relative = match normalize_relative(query.path.as_deref()) {
        Ok(path) => path,
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let base = state.storage_root.join(&base_relative);
    let needle = query.q.to_lowercase();
    let mut results = Vec::new();
    for entry in WalkDir::new(base)
        .into_iter()
        .filter_entry(|entry| entry.file_name() != INTERNAL_DIR)
        .flatten()
    {
        if entry.path() == state.storage_root.join(&base_relative) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if name.contains(&needle) {
            if let Ok(relative) = entry.path().strip_prefix(&state.storage_root) {
                if let Ok(item) = file_item(entry.path(), relative, &state) {
                    results.push(item);
                }
            }
        }
    }
    HttpResponse::Ok().json(serde_json::json!({ "items": results }))
}

#[post("/api/uploads")]
async fn create_upload(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<CreateUploadRequest>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    receiver_info!(
        "Create upload: filename={}, path={}, total_size={}, chunk_size={}",
        payload.filename,
        payload.path.as_deref().unwrap_or(""),
        payload.total_size,
        payload.chunk_size
    );
    if payload.filename.trim().is_empty()
        || payload.filename.contains('/')
        || payload.filename.contains('\\')
    {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "File name is invalid",
        );
    }
    if payload.chunk_size == 0 {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Chunk size is invalid",
        );
    }
    let relative_dir = match normalize_relative(payload.path.as_deref()) {
        Ok(path) => path,
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let upload_id = uuid::Uuid::new_v4().to_string();
    receiver_info!("Upload session created: {}", upload_id);
    let session = UploadSession {
        upload_id: upload_id.clone(),
        path: display_path(&relative_dir),
        filename: payload.filename.clone(),
        total_size: payload.total_size,
        chunk_size: payload.chunk_size,
        force: payload.force.unwrap_or(false),
        max_width: payload.max_width,
        max_height: payload.max_height,
        created_at: Utc::now(),
    };
    let dir = session_dir(&state, &upload_id);
    if fs::create_dir_all(dir.join("chunks")).is_err()
        || fs::write(
            session_meta_path(&state, &upload_id),
            serde_json::to_vec_pretty(&session).unwrap(),
        )
        .is_err()
    {
        receiver_error!("Failed to create upload session: {}", upload_id);
        return json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to create upload session",
        );
    }
    HttpResponse::Created().json(CreateUploadResponse { upload_id })
}

#[put("/api/uploads/{upload_id}/chunks/{index}")]
async fn put_chunk(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, u64)>,
    mut payload: web::Payload,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let (upload_id, index) = path.into_inner();
    receiver_info!("Uploading chunk {} for upload {}", index, upload_id);
    let session = match read_upload_session(&state, &upload_id) {
        Ok(session) => session,
        Err(response) => return response,
    };
    if index >= expected_chunk_count(&session) {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Chunk index is invalid",
        );
    }
    let chunk_path = session_dir(&state, &upload_id)
        .join("chunks")
        .join(index.to_string());
    let tmp_path = chunk_path.with_extension("tmp");
    let mut file = match tokio::fs::File::create(&tmp_path).await {
        Ok(file) => file,
        Err(_) => {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to write chunk",
            )
        }
    };
    let mut written = 0u64;
    while let Some(chunk) = payload.next().await {
        match chunk {
            Ok(bytes) => {
                written += bytes.len() as u64;
                if written > session.chunk_size {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    receiver_error!("Chunk {} too large for upload {}", index, upload_id);
                    return json_error(
                        actix_web::http::StatusCode::PAYLOAD_TOO_LARGE,
                        "Chunk is too large",
                    );
                }
                if file.write_all(&bytes).await.is_err() {
                    let _ = tokio::fs::remove_file(&tmp_path).await;
                    receiver_error!("Failed to write chunk {} for upload {}", index, upload_id);
                    return json_error(
                        actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                        "Unable to write chunk",
                    );
                }
            }
            Err(_) => {
                let _ = tokio::fs::remove_file(&tmp_path).await;
                receiver_error!("Chunk upload failed for {} chunk {}", upload_id, index);
                return json_error(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "Chunk upload failed",
                );
            }
        }
    }
    if tokio::fs::rename(&tmp_path, &chunk_path).await.is_err() {
        let _ = tokio::fs::remove_file(&tmp_path).await;
        receiver_error!("Failed to save chunk {} for upload {}", index, upload_id);
        return json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save chunk",
        );
    }
    receiver_debug!("Chunk {} uploaded for {}", index, upload_id);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[get("/api/uploads/{upload_id}")]
async fn get_upload(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let upload_id = path.into_inner();
    let session = match read_upload_session(&state, &upload_id) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let chunks = received_chunks(&state, &upload_id);
    let complete = chunks.len() as u64 == expected_chunk_count(&session);
    if complete {
        receiver_info!("Upload complete: {}", upload_id);
    }
    HttpResponse::Ok().json(UploadStatus {
        upload_id,
        complete,
        received_chunks: chunks,
    })
}

#[post("/api/uploads/{upload_id}/complete")]
async fn complete_upload(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let upload_id = path.into_inner();
    receiver_info!("Completing upload: {}", upload_id);
    let session = match read_upload_session(&state, &upload_id) {
        Ok(session) => session,
        Err(response) => return response,
    };
    let expected = expected_chunk_count(&session);
    let chunks = received_chunks(&state, &upload_id);
    if chunks.len() as u64 != expected
        || chunks.iter().copied().collect::<HashSet<_>>().len() as u64 != expected
    {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Upload is not complete",
        );
    }
    let relative_dir = match normalize_relative(Some(&session.path)) {
        Ok(path) => path,
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let relative_file = relative_dir.join(&session.filename);
    let final_path = state.storage_root.join(&relative_file);
    if final_path.exists() && !session.force {
        return json_error(actix_web::http::StatusCode::CONFLICT, "Name already exists");
    }
    if let Some(parent) = final_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to create folder",
            );
        }
    }
    let tmp_final = session_dir(&state, &upload_id).join("merged.tmp");
    let mut output = match File::create(&tmp_final) {
        Ok(file) => file,
        Err(_) => {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to merge upload",
            )
        }
    };
    for index in 0..expected {
        let chunk_path = session_dir(&state, &upload_id)
            .join("chunks")
            .join(index.to_string());
        let mut chunk = match File::open(chunk_path) {
            Ok(file) => file,
            Err(_) => {
                return json_error(
                    actix_web::http::StatusCode::BAD_REQUEST,
                    "Upload is not complete",
                )
            }
        };
        if std::io::copy(&mut chunk, &mut output).is_err() {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to merge upload",
            );
        }
    }
    drop(output);
    if fs::rename(&tmp_final, &final_path).is_err() {
        let _ = fs::remove_file(&tmp_final);
        receiver_error!("Failed to save uploaded file: {}", upload_id);
        return json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save file",
        );
    }
    receiver_info!("Upload completed: {} -> {}", upload_id, display_path(&relative_file));
    process_image(
        &final_path,
        &relative_file,
        &state,
        session.max_width,
        session.max_height,
    );
    let _ = fs::remove_dir_all(session_dir(&state, &upload_id));
    HttpResponse::Created().json(file_item(&final_path, &relative_file, &state).ok())
}

#[delete("/api/uploads/{upload_id}")]
async fn cancel_upload(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let upload_id = path.into_inner();
    receiver_info!("Cancelling upload: {}", upload_id);
    let _ = fs::remove_dir_all(session_dir(&state, &upload_id));
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[delete("/api/files")]
async fn delete_file(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PathQuery>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let relative = match normalize_relative(query.path.as_deref()) {
        Ok(path) if !path.as_os_str().is_empty() => path,
        Ok(_) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, "Path is required"),
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let full_path = state.storage_root.join(&relative);
    if !full_path.exists() {
        return json_error(actix_web::http::StatusCode::NOT_FOUND, "Path was not found");
    }
    receiver_info!("Deleting: {}", display_path(&relative));
    if read_settings(&state).trash_enabled {
        let id = uuid::Uuid::new_v4().to_string();
        receiver_info!("Moving to trash: {}", display_path(&relative));
        let trash_relative = PathBuf::from(INTERNAL_DIR)
            .join("trash")
            .join("items")
            .join(&id);
        let trash_full = state.storage_root.join(&trash_relative);
        if let Some(parent) = trash_full.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if fs::rename(&full_path, &trash_full).is_err() {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to move to trash",
            );
        }
        let mut entries = read_trash_index(&state);
        entries.push(TrashEntry {
            id,
            original_path: display_path(&relative),
            trash_path: display_path(&trash_relative),
            deleted_at: Utc::now(),
            kind: if trash_full.is_dir() {
                "folder"
            } else {
                "file"
            }
            .to_string(),
        });
        let _ = write_trash_index(&state, &entries);
    } else if full_path.is_dir() {
        receiver_info!("Permanently deleting folder: {}", display_path(&relative));
        if fs::remove_dir_all(full_path).is_err() {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete folder",
            );
        }
    } else {
        receiver_info!("Permanently deleting file: {}", display_path(&relative));
        if fs::remove_file(full_path).is_err() {
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to delete file",
            );
        }
    }
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[get("/api/files/download")]
async fn download(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<PathQuery>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let relative = match normalize_relative(query.path.as_deref()) {
        Ok(path) if !path.as_os_str().is_empty() => path,
        Ok(_) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, "Path is required"),
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    let full_path = state.storage_root.join(&relative);
    if !full_path.exists() {
        return json_error(actix_web::http::StatusCode::NOT_FOUND, "Path was not found");
    }
    receiver_info!("Downloading: {}", display_path(&relative));
    if full_path.is_file() {
        let mut data = Vec::new();
        if File::open(&full_path)
            .and_then(|mut file| file.read_to_end(&mut data))
            .is_err()
        {
            receiver_error!("Failed to read file: {}", display_path(&relative));
            return json_error(
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to read file",
            );
        }
        let filename = full_path
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("download");
        return HttpResponse::Ok()
            .insert_header((
                header::CONTENT_TYPE,
                mime_guess::from_path(&full_path)
                    .first_or_octet_stream()
                    .to_string(),
            ))
            .insert_header((
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{}\"", filename.replace('"', "")),
            ))
            .body(data);
    }
    let mut buffer = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buffer);
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for entry in WalkDir::new(&full_path).into_iter().flatten() {
            let path = entry.path();
            if path.file_name().and_then(|v| v.to_str()) == Some(INTERNAL_DIR) {
                continue;
            }
            let Ok(name) = path.strip_prefix(&full_path) else {
                continue;
            };
            if name.as_os_str().is_empty() {
                continue;
            }
            let name = display_path(name);
            if path.is_dir() {
                let _ = zip.add_directory(format!("{}/", name), options);
            } else if let Ok(mut file) = File::open(path) {
                let _ = zip.start_file(name, options);
                let _ = std::io::copy(&mut file, &mut zip);
            }
        }
        let _ = zip.finish();
    }
    let filename = full_path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("folder");
    receiver_info!("Downloading folder: {}.zip", filename);
    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "application/zip"))
        .insert_header((
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}.zip\"", filename.replace('"', "")),
        ))
        .body(buffer.into_inner())
}

#[get("/api/settings")]
async fn get_settings(req: HttpRequest, state: web::Data<AppState>) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    receiver_debug!("Getting settings");
    HttpResponse::Ok().json(read_settings(&state))
}

#[put("/api/settings")]
async fn put_settings(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<SettingsRequest>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    receiver_info!("Updating settings: trash_enabled={}", payload.trash_enabled);
    let settings = Settings {
        trash_enabled: payload.trash_enabled,
    };
    match write_settings(&state, &settings) {
        Ok(_) => HttpResponse::Ok().json(settings),
        Err(_) => json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to save settings",
        ),
    }
}

#[get("/api/trash")]
async fn list_trash(req: HttpRequest, state: web::Data<AppState>) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    receiver_info!("Listing trash items");
    HttpResponse::Ok().json(serde_json::json!({ "items": read_trash_index(&state) }))
}

#[post("/api/trash/restore")]
async fn restore_trash(
    req: HttpRequest,
    state: web::Data<AppState>,
    payload: web::Json<RestoreRequest>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let mut entries = read_trash_index(&state);
    let Some(pos) = entries.iter().position(|entry| entry.id == payload.id) else {
        return json_error(
            actix_web::http::StatusCode::NOT_FOUND,
            "Trash item was not found",
        );
    };
    let entry = entries.remove(pos);
    receiver_info!("Restoring trash item: {} -> {}", entry.id, entry.original_path);
    let source = state.storage_root.join(&entry.trash_path);
    let target = match storage_path(&state, Some(&entry.original_path)) {
        Ok(path) => path,
        Err(message) => return json_error(actix_web::http::StatusCode::BAD_REQUEST, message),
    };
    if target.exists() {
        return json_error(
            actix_web::http::StatusCode::CONFLICT,
            "Original path already exists",
        );
    }
    if let Some(parent) = target.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if fs::rename(source, target).is_err() {
        return json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to restore item",
        );
    }
    let _ = write_trash_index(&state, &entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[delete("/api/trash")]
async fn delete_trash(
    req: HttpRequest,
    state: web::Data<AppState>,
    query: web::Query<DeleteTrashQuery>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let mut entries = read_trash_index(&state);
    let Some(pos) = entries.iter().position(|entry| entry.id == query.id) else {
        return json_error(
            actix_web::http::StatusCode::NOT_FOUND,
            "Trash item was not found",
        );
    };
    let entry = entries.remove(pos);
    receiver_info!("Permanently deleting trash item: {}", entry.id);
    let full_path = state.storage_root.join(entry.trash_path);
    let result = if full_path.is_dir() {
        fs::remove_dir_all(full_path)
    } else {
        fs::remove_file(full_path)
    };
    if result.is_err() {
        return json_error(
            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Unable to delete trash item",
        );
    }
    let _ = write_trash_index(&state, &entries);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}

#[get("/api/thumbnails/{name}")]
async fn get_thumbnail(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> impl Responder {
    if let Err(response) = ensure_auth(&req, &state) {
        return response;
    }
    let name = path.into_inner();
    if name.contains('/') || name.contains('\\') || !name.ends_with(".jpg") {
        return json_error(
            actix_web::http::StatusCode::BAD_REQUEST,
            "Thumbnail name is invalid",
        );
    }
    receiver_info!("Serving thumbnail: {}", name);
    let full_path = thumbnails_root(&state).join(name);
    let mut data = Vec::new();
    if File::open(full_path)
        .and_then(|mut file| file.read_to_end(&mut data))
        .is_err()
    {
        return json_error(
            actix_web::http::StatusCode::NOT_FOUND,
            "Thumbnail was not found",
        );
    }
    HttpResponse::Ok()
        .insert_header((header::CONTENT_TYPE, "image/jpeg"))
        .body(data)
}

async fn spa(req: HttpRequest) -> impl Responder {
    let mut path = req.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }
    let asset = Frontend::get(path).or_else(|| Frontend::get("index.html"));
    match asset {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            HttpResponse::Ok()
                .insert_header((header::CONTENT_TYPE, mime.to_string()))
                .body(content.data.into_owned())
        }
        None => HttpResponse::NotFound().finish(),
    }
}

fn binary_storage_default() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("storage")
}

fn binary_runtime_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn resolve_storage_root() -> PathBuf {
    env::var("FILE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| binary_storage_default())
}

fn resolve_log_dir() -> PathBuf {
    env::var("LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| binary_runtime_dir().join("logs"))
}

fn parse_log_level(value: &str) -> Result<LevelFilter, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Ok(LevelFilter::Off),
        "error" => Ok(LevelFilter::Error),
        "warn" | "warning" => Ok(LevelFilter::Warn),
        "info" => Ok(LevelFilter::Info),
        "debug" => Ok(LevelFilter::Debug),
        "trace" => Ok(LevelFilter::Trace),
        other => Err(format!("unsupported log level: {other}")),
    }
}

impl FileLogger {
    fn open_file(log_dir: &Path, current_date: &str) -> std::io::Result<File> {
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!("receiver-{current_date}.log")))
    }
    
    fn enabled(&self, level: Level) -> bool {
        self.level >= level.to_level_filter()
    }

    fn write(&self, level: Level, target: &str, message: fmt::Arguments<'_>) {
        if !self.enabled(level) {
            return;
        }

        let now = Local::now();
        let current_date = now.format("%Y-%m-%d").to_string();
        let timestamp = now.format("%Y-%m-%d %H:%M:%S");

        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => return,
        };

        if state.file.is_none() || state.current_date != current_date {
            match Self::open_file(&self.log_dir, &current_date) {
                Ok(file) => {
                    state.current_date = current_date;
                    state.file = Some(file);
                }
                Err(_) => return,
            }
        }

        if let Some(file) = state.file.as_mut() {
            let _ = writeln!(
                file,
                "{} {:<5} [{}] {}",
                timestamp,
                level,
                target,
                message
            );
            let _ = file.flush();
        }
    }
}

fn write_log_line(level: Level, target: &str, message: fmt::Arguments<'_>) {
    if let Some(logger) = FILE_LOGGER.get() {
        logger.write(level, target, message);
    }
}

fn init_logger() -> std::io::Result<()> {
    let log_dir = resolve_log_dir();
    fs::create_dir_all(&log_dir)?;
    let current_date = Local::now().format("%Y-%m-%d").to_string();
    FileLogger::open_file(&log_dir, &current_date)?;
    let log_level = env::var("APP_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    let level = parse_log_level(&log_level)
        .map_err(|err| std::io::Error::other(format!("invalid log level: {err}")))?;
    let logger = FileLogger {
        level,
        log_dir,
        state: Mutex::new(LoggerState {
            current_date: String::new(),
            file: None,
        }),
    };
    FILE_LOGGER
        .set(logger)
        .map_err(|_| std::io::Error::other("failed to initialize logger"))?;
    Ok(())
}

fn build_state() -> std::io::Result<AppState> {
    let username = env::var("APP_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password = env::var("APP_PASSWORD").unwrap_or_else(|_| "admin".to_string());
    let storage_root = resolve_storage_root();
    receiver_info!("Storage root: {:?}", storage_root);
    fs::create_dir_all(&storage_root)?;
    let internal_root = storage_root.join(INTERNAL_DIR);
    fs::create_dir_all(internal_root.join("trash").join("items"))?;
    fs::create_dir_all(internal_root.join("uploads"))?;
    fs::create_dir_all(internal_root.join("thumbnails"))?;
    let state = AppState {
        username,
        password,
        storage_root,
        internal_root,
        sessions: Arc::new(Mutex::new(HashSet::new())),
    };
    if !settings_path(&state).exists() {
        write_settings(
            &state,
            &Settings {
                trash_enabled: false,
            },
        )?;
    }
    if !trash_index_path(&state).exists() {
        write_trash_index(&state, &[])?;
    }
    Ok(state)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    init_logger()?;
    let state = build_state()?;
    cleanup_trash(state.clone());
    let cleanup_state = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        loop {
            interval.tick().await;
            cleanup_trash(cleanup_state.clone());
        }
    });
    let bind = env::var("BIND_ADDRESS").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    receiver_info!("Starting receiver on {}", bind);
    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(state.clone()))
            .service(login)
            .service(logout)
            .service(me)
            .service(list_files)
            .service(create_folder)
            .service(search_files)
            .service(create_upload)
            .service(put_chunk)
            .service(get_upload)
            .service(complete_upload)
            .service(cancel_upload)
            .service(delete_file)
            .service(download)
            .service(get_settings)
            .service(put_settings)
            .service(list_trash)
            .service(restore_trash)
            .service(delete_trash)
            .service(get_thumbnail)
            .default_service(web::get().to(spa))
    })
    .bind(bind)?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_path_traversal() {
        assert!(normalize_relative(Some("../secret")).is_err());
        assert!(normalize_relative(Some("/tmp")).is_err());
        assert!(normalize_relative(Some(".receiver/file")).is_err());
    }

    #[test]
    fn accepts_clean_relative_path() {
        assert_eq!(
            normalize_relative(Some("photos/2026")).unwrap(),
            PathBuf::from("photos").join("2026")
        );
    }

    #[test]
    fn expected_chunks_rounds_up() {
        let session = UploadSession {
            upload_id: "x".to_string(),
            path: "".to_string(),
            filename: "a.bin".to_string(),
            total_size: 11,
            chunk_size: 5,
            force: false,
            max_width: None,
            max_height: None,
            created_at: Utc::now(),
        };
        assert_eq!(expected_chunk_count(&session), 3);
    }
}
