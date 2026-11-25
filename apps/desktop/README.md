# ATOM desktop shell (Tauri)

The desktop app runs the same Rust core and HTTP API as the browser build. On
startup it binds the API to an ephemeral loopback port and opens a window pointed
at that origin, so there is one implementation of the product.

## Prerequisites

Tauri needs platform GUI toolchains that are **not** installed in the Linux cloud
image used to build the rest of this repository:

- **Linux:** `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`,
  `librsvg2-dev`, `patchelf`, plus a C toolchain.
- **macOS:** Xcode command line tools.
- **Windows:** WebView2 runtime and the MSVC build tools.

Because of that, `apps/desktop` is excluded from the root Cargo workspace: the
portable crates, API and CLI build and test without any GUI dependency.

## Build the UI first

```bash
cd frontend
bun install
bun run build          # writes frontend/dist, which the API and the shell serve
```

## Run

```bash
cd apps/desktop
cargo run --release
```

Optional: point ATOM at an existing llama.cpp build without opening Settings.

```bash
ATOM_HOME="$HOME/.atom" \
LLAMA_SERVER=/opt/llama.cpp/build/bin/llama-server \
cargo run --release
```

(`LLAMA_SERVER` is only an example; ATOM reads binary paths from its config file
or PATH. Settings writes them for you.)

## Data location

`ATOM_HOME` if set, otherwise `$HOME/.atom`:

```
config.json          settings
library.json         model records with checksums
models/              imported GGUF files
compressed/          quantization outputs and per-run logs
benchmarks/          comparison JSON + Markdown reports
conversations/       chat transcripts
```

## Status

The shell compiles against Tauri 2 and is not exercised in CI here because the
GUI libraries are absent in this environment. The API and UI it hosts are built,
unit-tested and run from `apps/api`.
