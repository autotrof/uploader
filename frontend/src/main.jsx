import React, { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import {
  ArrowLeft,
  Download,
  File as FileIcon,
  FileSpreadsheet,
  FileText,
  Folder,
  FolderUp,
  Home,
  LogOut,
  Pencil,
  Plus,
  RefreshCw,
  RotateCcw,
  Search,
  Settings,
  Trash2,
  Upload,
  User,
  X
} from 'lucide-react';
import './styles.css';

const CHUNK_SIZE = 5 * 1024 * 1024;
const VIDEO_THUMBNAIL_TIME = 1;
const VIDEO_THUMBNAIL_MAX_EDGE = 512;
const OVERWRITE_CONFIRMED = 'overwrite_confirmed';

function appBasePath() {
  const path = window.location.pathname.replace(/\/+$/, '');
  return path === '/' ? '' : path;
}

function appUrl(path) {
  if (!path.startsWith('/')) return path;
  return `${appBasePath()}${path}`;
}

async function api(path, options = {}) {
  const response = await fetch(appUrl(path), {
    credentials: 'include',
    headers: {
      ...(options.body instanceof Blob ? {} : { 'Content-Type': 'application/json' }),
      ...(options.headers || {})
    },
    ...options
  });
  if (!response.ok) {
    let message = response.statusText;
    try {
      const body = await response.json();
      message = body.error || message;
    } catch {
      // Keep browser status text.
    }
    const error = new Error(message);
    error.status = response.status;
    throw error;
  }
  const type = response.headers.get('content-type') || '';
  return type.includes('application/json') ? response.json() : response;
}

function joinPath(base, name) {
  return [base, name].filter(Boolean).join('/');
}

function parentPath(path) {
  const parts = path.split('/').filter(Boolean);
  parts.pop();
  return parts.join('/');
}

function formatSize(value) {
  if (!value) return '-';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = value;
  let index = 0;
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024;
    index += 1;
  }
  return `${size.toFixed(index === 0 ? 0 : 1)} ${units[index]}`;
}

function formatDate(value) {
  if (!value) return '-';
  return new Date(value).toLocaleString();
}

function isVideoFile(file) {
  return file.type.startsWith('video/');
}

async function createVideoThumbnail(file) {
  const objectUrl = URL.createObjectURL(file);
  const video = document.createElement('video');
  video.preload = 'metadata';
  video.muted = true;
  video.playsInline = true;
  video.src = objectUrl;

  try {
    await new Promise((resolve, reject) => {
      video.onloadedmetadata = () => resolve();
      video.onerror = () => reject(new Error('Video tidak bisa dibaca.'));
    });

    const targetTime = Number.isFinite(video.duration)
      ? Math.min(VIDEO_THUMBNAIL_TIME, Math.max(video.duration - 0.1, 0))
      : 0;

    if (targetTime > 0) {
      await new Promise((resolve, reject) => {
        video.onseeked = () => resolve();
        video.onerror = () => reject(new Error('Frame video tidak bisa diambil.'));
        video.currentTime = targetTime;
      });
    }

    const width = video.videoWidth;
    const height = video.videoHeight;
    if (!width || !height) {
      throw new Error('Ukuran video tidak valid.');
    }

    const scale = Math.min(1, VIDEO_THUMBNAIL_MAX_EDGE / Math.max(width, height));
    const canvas = document.createElement('canvas');
    canvas.width = Math.max(1, Math.round(width * scale));
    canvas.height = Math.max(1, Math.round(height * scale));
    const context = canvas.getContext('2d');
    if (!context) {
      throw new Error('Canvas tidak tersedia.');
    }
    context.drawImage(video, 0, 0, canvas.width, canvas.height);

    const blob = await new Promise((resolve, reject) => {
      canvas.toBlob(
        (result) => {
          if (result) {
            resolve(result);
          } else {
            reject(new Error('Thumbnail video tidak bisa dibuat.'));
          }
        },
        'image/jpeg',
        0.85
      );
    });

    return blob;
  } finally {
    URL.revokeObjectURL(objectUrl);
    video.removeAttribute('src');
    video.load();
  }
}

function itemIcon(item, size = 20) {
  if (item.thumbnail) return <img src={appUrl(item.thumbnail)} alt="" className="item-thumb" />;
  if (item.kind === 'folder') return <Folder className="folder-glyph" size={size} />;
  if (item.name.toLowerCase().endsWith('.xlsx')) return <FileSpreadsheet className="sheet-glyph" size={size} />;
  if (item.name.toLowerCase().endsWith('.zip')) return <FileText className="doc-glyph" size={size} />;
  return <FileIcon className="doc-glyph" size={size} />;
}

function createUploadItemId() {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function uploadLabel(prepared) {
  return prepared.relativePath || prepared.file.name;
}

async function fileFromDroppedEntry(entry) {
  return new Promise((resolve, reject) => {
    entry.file(
      (file) => resolve({ file, relativePath: entry.fullPath.replace(/^\/+/, '') }),
      () => reject(new Error('File tidak bisa dibaca.'))
    );
  });
}

async function readDroppedDirectoryEntries(reader) {
  const entries = [];
  while (true) {
    const chunk = await new Promise((resolve, reject) => {
      reader.readEntries(resolve, () => reject(new Error('Folder tidak bisa dibaca.')));
    });
    if (!chunk.length) return entries;
    entries.push(...chunk);
  }
}

async function collectDroppedEntry(entry, files, folders) {
  const relativePath = entry.fullPath.replace(/^\/+/, '');
  if (entry.isFile) {
    files.push(await fileFromDroppedEntry(entry));
    return;
  }
  if (!entry.isDirectory) return;

  if (relativePath) {
    folders.push(relativePath);
  }
  const children = await readDroppedDirectoryEntries(entry.createReader());
  for (const child of children) {
    await collectDroppedEntry(child, files, folders);
  }
}

async function collectDroppedItems(dataTransfer) {
  const items = Array.from(dataTransfer?.items || []);
  if (!items.length || !items.some((item) => typeof item.webkitGetAsEntry === 'function')) {
    return null;
  }

  const files = [];
  const folders = [];
  for (const item of items) {
    const entry = item.webkitGetAsEntry();
    if (entry) {
      await collectDroppedEntry(entry, files, folders);
      continue;
    }
    const file = item.kind === 'file' ? item.getAsFile() : null;
    if (file) {
      files.push({ file, relativePath: '' });
    }
  }
  return { files, folders };
}

function Login({ onLogin }) {
  const [form, setForm] = useState({ username: '', password: '' });
  const [error, setError] = useState('');

  async function submit(event) {
    event.preventDefault();
    setError('');
    try {
      await api('/api/login', { method: 'POST', body: JSON.stringify(form) });
      onLogin();
    } catch {
      setError('Username atau password salah.');
    }
  }

  return (
    <main className="login-screen">
      <form className="login-panel" onSubmit={submit}>
        <div className="login-brand">
          <span><Folder size={20} /></span>
          <div>
            <p>Receiver</p>
            <h1>Masuk</h1>
          </div>
        </div>
        <label>
          Username
          <input value={form.username} onChange={(event) => setForm({ ...form, username: event.target.value })} autoFocus />
        </label>
        <label>
          Password
          <input type="password" value={form.password} onChange={(event) => setForm({ ...form, password: event.target.value })} />
        </label>
        {error && <p className="error">{error}</p>}
        <button className="primary" type="submit">Masuk</button>
      </form>
    </main>
  );
}

function App() {
  const [user, setUser] = useState(null);
  const [checking, setChecking] = useState(true);
  const [path, setPath] = useState('');
  const [items, setItems] = useState([]);
  const [query, setQuery] = useState('');
  const [searchResults, setSearchResults] = useState(null);
  const [message, setMessage] = useState('');
  const [uploadItems, setUploadItems] = useState([]);
  const [dragActive, setDragActive] = useState(false);
  const [view, setView] = useState('files');
  const [settings, setSettings] = useState({ trash_enabled: false });
  const [trash, setTrash] = useState([]);
  const [selectedPaths, setSelectedPaths] = useState([]);
  const dragDepth = useRef(0);
  const fileInputRef = useRef(null);
  const folderInputRef = useRef(null);
  const uploadControllers = useRef(new Map());

  async function loadMe() {
    try {
      const data = await api('/api/me');
      setUser(data);
    } catch {
      setUser(null);
    } finally {
      setChecking(false);
    }
  }

  async function loadFiles(nextPath = path) {
    const data = await api(`/api/files?path=${encodeURIComponent(nextPath)}`);
    setPath(data.path);
    setItems(data.items || []);
    setSearchResults(null);
    setSelectedPaths([]);
  }

  async function loadSettings() {
    setSettings(await api('/api/settings'));
  }

  async function loadTrash() {
    const data = await api('/api/trash');
    setTrash(data.items || []);
  }

  useEffect(() => {
    loadMe();
  }, []);

  useEffect(() => {
    if (user) {
      loadFiles('');
      loadSettings();
    }
  }, [user]);

  useEffect(() => {
    if (view === 'trash') loadTrash();
  }, [view]);

  useEffect(() => {
    if (!folderInputRef.current) return;
    folderInputRef.current.setAttribute('webkitdirectory', '');
    folderInputRef.current.setAttribute('directory', '');
  }, []);

  async function logout() {
    await api('/api/logout', { method: 'POST' });
    setUser(null);
  }

  async function createFolder() {
    const nextName = prompt('Nama folder baru:', '')?.trim();
    if (!nextName) return;
    try {
      await api(`/api/folders?path=${encodeURIComponent(joinPath(path, nextName))}&force=1`, { method: 'POST' });
      await loadFiles();
    } catch (err) {
      setMessage(err.message);
    }
  }

  async function renameItem(item) {
    const newName = prompt('Nama baru:', item.name)?.trim();
    if (!newName || newName === item.name) return;
    try {
      await api('/api/files/rename', {
        method: 'PUT',
        body: JSON.stringify({ path: item.path, new_name: newName })
      });
      await loadFiles();
    } catch (err) {
      setMessage(err.message);
    }
  }

  async function deleteItem(item) {
    if (!confirm(`Hapus "${item.name}"?`)) return;
    try {
      await api(`/api/files?path=${encodeURIComponent(item.path)}`, { method: 'DELETE' });
      await loadFiles();
    } catch (err) {
      setMessage(err.message);
    }
  }

  function downloadItem(item) {
    window.location.href = appUrl(`/api/files/download?path=${encodeURIComponent(item.path)}`);
  }

  function toggleSelectedPath(itemPath) {
    setSelectedPaths((current) =>
      current.includes(itemPath) ? current.filter((value) => value !== itemPath) : [...current, itemPath]
    );
  }

  function toggleSelectAllShownItems() {
    const shownPaths = shownItems.map((item) => item.path);
    const allSelected = shownPaths.length > 0 && shownPaths.every((itemPath) => selectedPaths.includes(itemPath));
    setSelectedPaths(allSelected ? [] : shownPaths);
  }

  async function deleteSelectedItems() {
    if (!selectedPaths.length) return;
    if (!confirm(`Hapus ${selectedPaths.length} item?`)) return;
    const errors = [];
    for (const itemPath of selectedPaths) {
      try {
        await api(`/api/files?path=${encodeURIComponent(itemPath)}`, { method: 'DELETE' });
      } catch (err) {
        errors.push(`"${itemPath}" gagal dihapus: ${err.message}`);
      }
    }
    await loadFiles();
    if (errors.length) {
      setMessage(errors.join(' '));
    }
  }

  async function runSearch(event) {
    event.preventDefault();
    if (!query.trim()) {
      setSearchResults(null);
      return;
    }
    try {
      const data = await api(`/api/search?q=${encodeURIComponent(query.trim())}&path=${encodeURIComponent(path)}`);
      setSearchResults(data.items || []);
    } catch (err) {
      setMessage(err.message);
    }
  }

  async function createUploadSessions(preparedFiles, options) {
    if (preparedFiles.length === 1) {
      const prepared = preparedFiles[0];
      const upload = await api('/api/uploads', {
        method: 'POST',
        body: JSON.stringify({
          path: prepared.targetPath,
          filename: prepared.filename,
          total_size: prepared.file.size,
          chunk_size: CHUNK_SIZE,
          force: options.force,
          max_width: null,
          max_height: null,
          thumbnail_size: prepared.thumbnail?.size ?? null,
          thumbnail_content_type: prepared.thumbnail?.type ?? null
        })
      });
      return [upload];
    }
    const batch = await api('/api/uploads/batch', {
      method: 'POST',
      body: JSON.stringify({
        files: preparedFiles.map((prepared) => ({
          path: prepared.targetPath,
          filename: prepared.filename,
          total_size: prepared.file.size,
          chunk_size: CHUNK_SIZE,
          force: options.force,
          max_width: null,
          max_height: null,
          thumbnail_size: prepared.thumbnail?.size ?? null,
          thumbnail_content_type: prepared.thumbnail?.type ?? null
        }))
      })
    });
    return batch.uploads || [];
  }

  function updateUploadItem(uploadItemId, updates) {
    setUploadItems((current) =>
      current.map((item) => (item.id === uploadItemId ? { ...item, ...updates } : item))
    );
  }

  function removeUploadItem(uploadItemId) {
    setUploadItems((current) => current.filter((item) => item.id !== uploadItemId));
  }

  function setUploadProgress(uploadItemId, prepared, uploadedChunks, totalChunks) {
    const progress = totalChunks === 0 ? 100 : Math.round((uploadedChunks / totalChunks) * 100);
    updateUploadItem(uploadItemId, {
      name: uploadLabel(prepared),
      progress,
      status: 'uploading'
    });
  }

  async function uploadToSession(prepared, uploadId, uploadItemId, signal) {
    const { file, thumbnail } = prepared;
    const status = await api(`/api/uploads/${uploadId}`, { signal });
    const done = new Set(status.received_chunks || []);
    const total = Math.ceil(file.size / CHUNK_SIZE);
    if (total > 0) {
      setUploadProgress(uploadItemId, prepared, done.size, total);
    }
    if (total === 0) {
      setUploadProgress(uploadItemId, prepared, 1, 0);
    }
    for (let index = 0; index < total; index += 1) {
      if (done.has(index)) continue;
      const start = index * CHUNK_SIZE;
      const chunk = file.slice(start, Math.min(file.size, start + CHUNK_SIZE));
      await api(`/api/uploads/${uploadId}/chunks/${index}`, {
        method: 'PUT',
        body: chunk,
        headers: { 'Content-Type': 'application/octet-stream' },
        signal
      });
      setUploadProgress(uploadItemId, prepared, index + 1, total);
    }
    if (thumbnail) {
      await api(`/api/uploads/${uploadId}/thumbnail`, {
        method: 'PUT',
        body: thumbnail,
        headers: { 'Content-Type': thumbnail.type || 'image/jpeg' },
        signal
      });
    }
    await api(`/api/uploads/${uploadId}/complete`, { method: 'POST', signal });
  }

  async function cancelUpload(uploadId) {
    try {
      await api(`/api/uploads/${uploadId}`, { method: 'DELETE' });
    } catch {
      // Ignore cleanup errors.
    }
  }

  async function uploadSingleFile(prepared, uploadItemId, preparedUploadId, force, signal) {
    const { file } = prepared;
    let uploadId = preparedUploadId;
    try {
      if (!uploadId) {
        const uploads = await createUploadSessions([prepared], { force });
        uploadId = uploads[0]?.upload_id;
      }
      if (!uploadId) {
        throw new Error('Sesi upload tidak bisa dibuat.');
      }
      updateUploadItem(uploadItemId, { uploadId, status: 'uploading', progress: 0 });
      setUploadProgress(uploadItemId, prepared, 0, Math.ceil(file.size / CHUNK_SIZE));
      await uploadToSession(prepared, uploadId, uploadItemId, signal);
      updateUploadItem(uploadItemId, { status: 'done', progress: 100, uploadId: null });
      removeUploadItem(uploadItemId);
    } catch (err) {
      if (uploadId) {
        await cancelUpload(uploadId);
      }
      if (err?.name === 'AbortError') {
        updateUploadItem(uploadItemId, { status: 'cancelled', uploadId: null });
        return;
      }
      if (err.status === 409 && !force && confirm(`"${uploadLabel(prepared)}" sudah ada. Ganti file yang lama?`)) {
        updateUploadItem(uploadItemId, { status: 'retrying', uploadId: null, progress: 0 });
        await uploadSingleFile(prepared, uploadItemId, null, true, signal);
        return;
      }
      updateUploadItem(uploadItemId, { status: 'error', error: err.message, uploadId: null });
      throw err;
    }
  }

  function prepareSelectedFiles(list) {
    return Array.from(list || [])
      .map((item) => {
        const file = item instanceof File ? item : item?.file;
        if (!(file instanceof File)) return null;
        const relativePath = item instanceof File ? file.webkitRelativePath || '' : item.relativePath || '';
        const parts = relativePath ? relativePath.split('/').filter(Boolean) : [];
        const filename = parts.length ? parts[parts.length - 1] : file.name;
        const targetPath = parts.length > 1 ? joinPath(path, parts.slice(0, -1).join('/')) : path;
        return {
          id: createUploadItemId(),
          file,
          filename,
          targetPath,
          relativePath
        };
      })
      .filter(Boolean);
  }

  async function createDroppedFolders(folders) {
    const uniqueFolders = [...new Set((folders || []).map((folder) => folder.split('/').filter(Boolean).join('/')).filter(Boolean))];
    for (const folder of uniqueFolders) {
      await api(`/api/folders?path=${encodeURIComponent(joinPath(path, folder))}&force=1`, { method: 'POST' });
    }
  }

  async function handleSelectedFiles(list, folders = []) {
    const preparedFiles = prepareSelectedFiles(list);
    const files = preparedFiles.map((item) => item.file);
    if (!files.length && !folders.length) return;
    setMessage('');
    const warnings = [];
    let sessions = [];
    try {
      await createDroppedFolders(folders);
      if (!files.length) {
        await loadFiles();
        return;
      }
      const initialUploadItems = preparedFiles.map((prepared) => ({
        id: prepared.id,
        name: uploadLabel(prepared),
        progress: 0,
        status: 'preparing',
        uploadId: null,
        error: ''
      }));
      setUploadItems((current) => [...initialUploadItems, ...current]);
      for (const prepared of preparedFiles) {
        let thumbnail = null;
        if (isVideoFile(prepared.file)) {
          try {
            thumbnail = await createVideoThumbnail(prepared.file);
          } catch {
            warnings.push(`Thumbnail video untuk "${uploadLabel(prepared)}" tidak berhasil dibuat.`);
          }
        }
        prepared.thumbnail = thumbnail;
      }
      sessions = await createUploadSessions(preparedFiles, { force: false });
      if (sessions.length !== preparedFiles.length) {
        throw new Error('Jumlah sesi upload tidak sesuai.');
      }
      await Promise.allSettled(
        preparedFiles.map((prepared, index) => {
          const controller = new AbortController();
          uploadControllers.current.set(prepared.id, controller);
          updateUploadItem(prepared.id, { uploadId: sessions[index]?.upload_id, status: 'uploading' });
          return uploadSingleFile(prepared, prepared.id, sessions[index]?.upload_id, false, controller.signal)
            .finally(() => {
              uploadControllers.current.delete(prepared.id);
            });
        })
      );
      await loadFiles();
    } catch (err) {
      setMessage(err.message);
      for (const session of sessions) {
        if (session?.upload_id) {
          await cancelUpload(session.upload_id);
        }
      }
      return;
    }
    if (warnings.length) {
      setMessage(warnings.join(' '));
    }
  }

  async function onPickFile(event) {
    const files = event.target.files;
    event.target.value = '';
    await handleSelectedFiles(files);
  }

  async function onPickFolder(event) {
    const files = event.target.files;
    event.target.value = '';
    await handleSelectedFiles(files);
  }

  async function cancelUploadItem(uploadItemId) {
    const controller = uploadControllers.current.get(uploadItemId);
    if (controller) {
      controller.abort();
      return;
    }
    setUploadItems((current) => current.filter((item) => item.id !== uploadItemId));
  }

  function onDragEnter(event) {
    event.preventDefault();
    dragDepth.current += 1;
    setDragActive(true);
  }

  function onDragOver(event) {
    event.preventDefault();
    if (!dragActive) setDragActive(true);
  }

  function onDragLeave(event) {
    event.preventDefault();
    dragDepth.current = Math.max(0, dragDepth.current - 1);
    if (dragDepth.current === 0) {
      setDragActive(false);
    }
  }

  async function onDropFiles(event) {
    event.preventDefault();
    dragDepth.current = 0;
    setDragActive(false);
    try {
      const dropped = await collectDroppedItems(event.dataTransfer);
      if (dropped) {
        await handleSelectedFiles(dropped.files, dropped.folders);
        return;
      }
      if (event.dataTransfer?.files?.length) {
        await handleSelectedFiles(event.dataTransfer.files);
      }
    } catch (err) {
      setMessage(err.message);
    }
  }

  async function toggleTrash() {
    try {
      const next = { trash_enabled: !settings.trash_enabled };
      setSettings(await api('/api/settings', { method: 'PUT', body: JSON.stringify(next) }));
    } catch (err) {
      setMessage(err.message);
    }
  }

  async function restoreTrash(id) {
    try {
      await api('/api/trash/restore', { method: 'POST', body: JSON.stringify({ id }) });
      await loadTrash();
      await loadFiles();
    } catch (err) {
      setMessage(err.message);
    }
  }

  async function removeTrash(id) {
    if (!confirm('Hapus permanen item ini?')) return;
    try {
      await api(`/api/trash?id=${encodeURIComponent(id)}`, { method: 'DELETE' });
      await loadTrash();
    } catch (err) {
      setMessage(err.message);
    }
  }

  const shownItems = searchResults || items;
  const title = path || 'Semua File';
  const breadcrumbItems = path
    ? path.split('/').filter(Boolean).map((part, index, parts) => ({
      name: part,
      path: parts.slice(0, index + 1).join('/')
    }))
    : [];
  const allShownSelected = shownItems.length > 0 && shownItems.every((item) => selectedPaths.includes(item.path));

  if (checking) return <div className="loading">Memuat...</div>;
  if (!user) return <Login onLogin={loadMe} />;

  return (
    <main className="app-shell">
      <header className="topbar">
        <div className="brand-block">
          <span className="brand-icon"><Folder size={18} /></span>
          <div>
            <strong>Receiver</strong>
            <small>{title}</small>
          </div>
        </div>
        <div className="header-actions">
          <span className="user-badge"><User size={14} /> {user.username}</span>
          <button className="ghost-button" onClick={logout}><LogOut size={16} /> Keluar</button>
        </div>
      </header>

      <div className="app-body">
        <aside className="sidebar">
          <button className={view === 'files' ? 'sidebar-link active' : 'sidebar-link'} onClick={() => setView('files')}>
            <Home size={16} /> File
          </button>
          <button className={view === 'trash' ? 'sidebar-link active' : 'sidebar-link'} onClick={() => setView('trash')}>
            <Trash2 size={16} /> Sampah
          </button>
          <button className={view === 'settings' ? 'sidebar-link active' : 'sidebar-link'} onClick={() => setView('settings')}>
            <Settings size={16} /> Pengaturan
          </button>
        </aside>

        <section className="workspace">
          {message && (
            <div className="toast">
              <span>{message}</span>
              <button onClick={() => setMessage('')}><X size={14} /></button>
            </div>
          )}

          {view === 'files' && (
            <>
              <div className="toolbar">
                <div className="toolbar-left">
                  <button className="ghost-button" disabled={!path} onClick={() => loadFiles(parentPath(path))}>
                    <ArrowLeft size={16} /> Kembali
                  </button>
                  <button className="ghost-button" onClick={() => loadFiles()}>
                    <RefreshCw size={16} /> Muat ulang
                  </button>
                </div>
                <div className="toolbar-right">
                  <button className="ghost-button" disabled={!selectedPaths.length} onClick={deleteSelectedItems}>
                    <Trash2 size={16} /> Hapus pilihan
                  </button>
                  <button className="ghost-button" onClick={createFolder}>
                    <Plus size={16} /> Folder
                  </button>
                  <label className="ghost-button upload-inline">
                    <FolderUp size={16} /> Upload Folder
                    <input
                      ref={folderInputRef}
                      type="file"
                      hidden
                      multiple
                      webkitdirectory=""
                      directory=""
                      onChange={onPickFolder}
                    />
                  </label>
                  <label className="primary upload-inline">
                    <Upload size={16} /> Upload File
                    <input ref={fileInputRef} type="file" hidden multiple onChange={onPickFile} />
                  </label>
                </div>
              </div>

              <nav className="breadcrumb" aria-label="Posisi folder saat ini">
                <button className={!path ? 'active' : ''} onClick={() => loadFiles('')}>
                  <Home size={14} /> Semua File
                </button>
                {breadcrumbItems.map((item) => (
                  <React.Fragment key={item.path}>
                    <span>/</span>
                    <button
                      className={item.path === path ? 'active' : ''}
                      onClick={() => loadFiles(item.path)}
                    >
                      {item.name}
                    </button>
                  </React.Fragment>
                ))}
              </nav>

              <form className="search-bar" onSubmit={runSearch}>
                <Search size={16} />
                <input
                  placeholder="Cari file atau folder"
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
                {searchResults && (
                  <button type="button" className="ghost-button" onClick={() => setSearchResults(null)}>
                    Tampilkan semua
                  </button>
                )}
              </form>

              {!!uploadItems.length && (
                <div className="upload-list">
                  {uploadItems.map((upload) => (
                    <div className={`progress ${upload.status}`} key={upload.id}>
                      <span>{upload.name}</span>
                      <div><i style={{ width: `${upload.progress}%` }} /></div>
                      <b>
                        {upload.status === 'done' && 'Selesai'}
                        {upload.status === 'cancelled' && 'Batal'}
                        {upload.status === 'error' && 'Gagal'}
                        {upload.status === 'preparing' && 'Siapkan'}
                        {upload.status === 'retrying' && 'Ulang'}
                        {upload.status === 'uploading' && `${upload.progress}%`}
                      </b>
                      <button className="progress-close" title="Tutup" onClick={() => cancelUploadItem(upload.id)}>
                        <X size={14} />
                      </button>
                    </div>
                  ))}
                </div>
              )}

              <section
                className={dragActive ? 'panel dropzone active' : 'panel dropzone'}
                onDragEnter={onDragEnter}
                onDragOver={onDragOver}
                onDragLeave={onDragLeave}
                onDrop={onDropFiles}
              >
                <div className="panel-head">
                  <div>
                    <h1>{title}</h1>
                    <p>{searchResults ? `${shownItems.length} hasil ditemukan` : `${shownItems.length} item`}</p>
                  </div>
                  <label className="check-all">
                    <input type="checkbox" checked={allShownSelected} onChange={toggleSelectAllShownItems} />
                    <span>Pilih semua</span>
                  </label>
                </div>

                <label className="dropzone-callout">
                  <input type="file" hidden multiple onChange={onPickFile} />
                  <Upload size={18} />
                  <span>Tarik file ke sini, upload file, atau upload folder</span>
                </label>

                <div className="item-list">
                  {shownItems.map((item) => (
                    <article className="item-row" key={item.path}>
                      <label className="item-check">
                        <input
                          type="checkbox"
                          checked={selectedPaths.includes(item.path)}
                          onChange={() => toggleSelectedPath(item.path)}
                        />
                      </label>
                      <button className="item-main" onClick={() => item.kind === 'folder' && loadFiles(item.path)}>
                        <span className="item-icon">{itemIcon(item)}</span>
                        <span className="item-copy">
                          <strong>{item.name}</strong>
                          <small>{item.kind === 'folder' ? 'Folder' : formatSize(item.size)}</small>
                        </span>
                      </button>
                      <span className="item-meta">{formatDate(item.modified)}</span>
                      <div className="row-actions">
                        <button title="Rename" onClick={() => renameItem(item)}><Pencil size={16} /></button>
                        <button title="Download" onClick={() => downloadItem(item)}><Download size={16} /></button>
                        <button title="Hapus" onClick={() => deleteItem(item)}><Trash2 size={16} /></button>
                      </div>
                    </article>
                  ))}
                  {!shownItems.length && <div className="empty-state">Belum ada file atau folder.</div>}
                </div>
              </section>
            </>
          )}

          {view === 'trash' && (
            <section className="panel">
              <div className="panel-head">
                <div>
                  <h1>Sampah</h1>
                  <p>{trash.length} item</p>
                </div>
                <button className="ghost-button" onClick={loadTrash}>
                  <RefreshCw size={16} /> Muat ulang
                </button>
              </div>

              <div className="item-list">
                {trash.map((item) => (
                  <article className="item-row" key={item.id}>
                    <div className="item-main static-row">
                      <span className="item-icon trash-icon"><Trash2 size={16} /></span>
                      <span className="item-copy">
                        <strong>{item.original_path}</strong>
                        <small>{item.kind === 'folder' ? 'Folder' : 'File'} • {formatDate(item.deleted_at)}</small>
                      </span>
                    </div>
                    <span className="item-meta" />
                    <div className="row-actions">
                      <button title="Pulihkan" onClick={() => restoreTrash(item.id)}><RotateCcw size={16} /></button>
                      <button title="Hapus permanen" onClick={() => removeTrash(item.id)}><Trash2 size={16} /></button>
                    </div>
                  </article>
                ))}
                {!trash.length && <div className="empty-state">Sampah kosong.</div>}
              </div>
            </section>
          )}

          {view === 'settings' && (
            <section className="panel settings-panel">
              <div className="panel-head">
                <div>
                  <h1>Pengaturan</h1>
                  <p>Pilihan saat menghapus file</p>
                </div>
              </div>
              <label className="toggle">
                <input type="checkbox" checked={settings.trash_enabled} onChange={toggleTrash} />
                <span />
                <div>
                  <strong>Gunakan sampah</strong>
                  <small>File yang dihapus bisa dipulihkan sampai 30 hari.</small>
                </div>
              </label>
            </section>
          )}
        </section>
      </div>
    </main>
  );
}

createRoot(document.getElementById('root')).render(<App />);
