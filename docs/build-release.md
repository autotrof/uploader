# Build and Release

Receiver is intended to be released as one Linux binary with frontend assets embedded.

## Local Build

```bash
cd frontend
npm ci
npm run build
cd ../backend
cargo build --release --target x86_64-unknown-linux-musl
```

For non-native musl targets such as `i686-unknown-linux-musl` and `aarch64-unknown-linux-musl`,
a native musl cross-compiler is required if you use plain `cargo build`.
If the target-specific musl gcc is not installed, use:

```bash
cargo zigbuild --release --target <target>
```

## Targets

- `i686-unknown-linux-musl`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-musl`

Install Rust targets before building:

```bash
rustup target add i686-unknown-linux-musl x86_64-unknown-linux-musl aarch64-unknown-linux-musl
```

The repository `build` script automatically uses `cargo zigbuild` for `i686-unknown-linux-musl`
and `aarch64-unknown-linux-musl` when the target musl gcc is unavailable and both `zig`
and `cargo-zigbuild` are installed.

The CI workflow builds all three targets and uploads the binaries as artifacts.
