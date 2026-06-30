# Storage Layout

Receiver stores managed files under `FILE_ROOT`. If `FILE_ROOT` is not set, it uses `storage/` next to the running binary.

Internal data is hidden under `.receiver`:

```text
storage/
  user-file.txt
  photos/
  .receiver/
    settings.json
    thumbnails/
    uploads/
    trash/
      index.json
      items/
```

The `.receiver` folder must not be shown in the UI, search results, or normal downloads.

## Path Safety

Only normal relative path components are allowed. Absolute paths, parent directory references, and `.receiver` are rejected.
