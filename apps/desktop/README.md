# ATOM desktop app (Windows + Linux)

This is a native desktop application, not a website. One executable contains the
Rust core, the HTTP API and the compiled React interface. On launch it binds the
API to an ephemeral `127.0.0.1` port and opens a native window on that origin, so
there is no browser tab, no separate server to start and no `frontend/dist`
folder to ship. Closing the window stops the app.

## Installers

| Platform | Artifact | Built by |
| --- | --- | --- |
| Windows 10/11 x64 | `ATOM_0.1.0_x64-setup.exe` (NSIS), `ATOM_0.1.0_x64_en-US.msi` | `scripts\build-desktop.ps1` |
| Linux x86_64 | `ATOM_0.1.0_amd64.deb`, `ATOM_0.1.0_amd64.AppImage` | `scripts/build-desktop.sh` |

The installers are produced from the same source on the corresponding OS. To get
both without owning a Windows machine, push a `v*` tag (or run the workflow
manually): [`.github/workflows/desktop.yml`](../../.github/workflows/desktop.yml)
builds on `windows-latest` and `ubuntu-22.04`, uploads the installers as
artifacts and attaches them to the GitHub release.

## Build on Windows

Prerequisites:

- **Rust (MSVC)** from <https://rustup.rs>, with the *Desktop development with
  C++* workload from Visual Studio Build Tools.
- **Bun** (<https://bun.sh>) and **Node.js** on `PATH` (`npx` runs the Tauri CLI).
- WebView2 runtime — preinstalled on Windows 10/11. The installer uses the
  download bootstrapper when it is missing.

```powershell
powershell -ExecutionPolicy Bypass -File scripts\build-desktop.ps1
# or pick targets:
powershell -ExecutionPolicy Bypass -File scripts\build-desktop.ps1 nsis
```

Installers appear in `apps\desktop\target\release\bundle\{nsis,msi}`.

## Build on Linux

Prerequisites (Ubuntu/Debian):

```bash
sudo apt install libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf build-essential
```

Fedora:

```bash
sudo dnf install webkit2gtk4.1-devel gtk3-devel libappindicator-gtk3-devel \
  librsvg2-devel patchelf
```

Then:

```bash
scripts/build-desktop.sh            # deb + AppImage
scripts/build-desktop.sh deb        # single target
```

Artifacts appear in `apps/desktop/target/release/bundle/{deb,appimage}`. Install
the `.deb` with `sudo apt install ./ATOM_0.1.0_amd64.deb`, or make the AppImage
executable and run it directly:

```bash
chmod +x ATOM_0.1.0_amd64.AppImage
./ATOM_0.1.0_amd64.AppImage
```

## Run from source (development)

```bash
cd frontend && bun install && bun run build
cd ../apps/desktop
cargo run --release
```

`cargo tauri dev` is not wired to Vite here because the shipped app serves the
compiled UI from its own process; rebuild the UI after frontend changes.

## Data location

`ATOM_HOME` if set, otherwise `~/.atom` (`%USERPROFILE%\.atom` on Windows):

```
config.json          settings
library.json         model records with checksums
models/              imported GGUF files
compressed/          quantization outputs and per-run logs
benchmarks/          comparison JSON + Markdown reports
conversations/       chat transcripts
micro/               ATOM Micro projects and datasets
exports/             Micro model/firmware exports
```

Uninstalling the app leaves this folder untouched; delete it to remove all data.

## Why the shell is a separate Cargo workspace

`apps/desktop` is excluded from the root workspace so the portable crates, API and
CLI build and test on machines without GUI libraries. The desktop crate is the
only place that depends on Tauri/WebKitGTK.
