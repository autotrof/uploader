import React, { useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import {
  ArrowLeft,
  Download,
  File as FileIcon,
  FileSpreadsheet,
  FileText,
  Folder,
  Home,
  LogOut,
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

async function api(path, options = {}) {
  const response = await fetch(path, {
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

function itemIcon(item, size = 20) {
  if (item.thumbnail) return <img src={item.thumbnail} alt="" className="item-thumb" />;
  if (item.kind === 'folder') return <Folder className="folder-glyph" size={size} />;
  if (item.name.toLowerCase().endsWith('.xlsx')) return <FileSpreadsheet className="sheet-glyph" size={size} />;
  if (item.name.toLowerCase().endsWith('.zip')) return <FileText className="doc-glyph" size={size} />;
  return <FileIcon className="doc-glyph" size={size} />;
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
  const [upload, setUpload] = useState(null);
  const [view, setView] = useState('files');
  const [settings, setSettings] = useState({ trash_enabled: false });
  const [trash, setTrash] = useState([]);

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
    window.location.href = `/api/files/download?path=${encodeURIComponent(item.path)}`;
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

  async function uploadFile(file, options) {
    const create = await api('/api/uploads', {
      method: 'POST',
      body: JSON.stringify({
        path,
        filename: file.name,
        total_size: file.size,
        chunk_size: CHUNK_SIZE,
        force: options.force,
        max_width: null,
        max_height: null
      })
    });
    const uploadId = create.upload_id;
    const status = await api(`/api/uploads/${uploadId}`);
    const done = new Set(status.received_chunks || []);
    const total = Math.ceil(file.size / CHUNK_SIZE);
    for (let index = 0; index < total; index += 1) {
      if (done.has(index)) continue;
      const start = index * CHUNK_SIZE;
      const chunk = file.slice(start, Math.min(file.size, start + CHUNK_SIZE));
      await api(`/api/uploads/${uploadId}/chunks/${index}`, {
        method: 'PUT',
        body: chunk,
        headers: { 'Content-Type': 'application/octet-stream' }
      });
      setUpload({ name: file.name, progress: Math.round(((index + 1) / total) * 100) });
    }
    await api(`/api/uploads/${uploadId}/complete`, { method: 'POST' });
    setUpload(null);
    await loadFiles();
  }

  async function onPickFile(event) {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    try {
      await uploadFile(file, { force: false });
    } catch (err) {
      if (err.status === 409 && confirm(`"${file.name}" sudah ada. Ganti file yang lama?`)) {
        try {
          await uploadFile(file, { force: true });
          return;
        } catch (retryError) {
          setUpload(null);
          setMessage(retryError.message);
          return;
        }
      }
      setUpload(null);
      setMessage(err.status === 409 ? 'Nama file sudah ada.' : err.message);
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
                  <button className="ghost-button" onClick={createFolder}>
                    <Plus size={16} /> Folder
                  </button>
                  <label className="primary upload-inline">
                    <Upload size={16} /> Upload
                    <input type="file" hidden onChange={onPickFile} />
                  </label>
                </div>
              </div>

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

              {upload && (
                <div className="progress">
                  <span>{upload.name}</span>
                  <div><i style={{ width: `${upload.progress}%` }} /></div>
                  <b>{upload.progress}%</b>
                </div>
              )}

              <section className="panel">
                <div className="panel-head">
                  <div>
                    <h1>{title}</h1>
                    <p>{searchResults ? `${shownItems.length} hasil ditemukan` : `${shownItems.length} item`}</p>
                  </div>
                </div>

                <div className="item-list">
                  {shownItems.map((item) => (
                    <article className="item-row" key={item.path}>
                      <button className="item-main" onClick={() => item.kind === 'folder' && loadFiles(item.path)}>
                        <span className="item-icon">{itemIcon(item)}</span>
                        <span className="item-copy">
                          <strong>{item.name}</strong>
                          <small>{item.kind === 'folder' ? 'Folder' : formatSize(item.size)}</small>
                        </span>
                      </button>
                      <span className="item-meta">{formatDate(item.modified)}</span>
                      <div className="row-actions">
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
