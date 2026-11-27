# Verification log

Environment: Linux cloud VM, 8 CPUs, 16 GiB RAM, Rust 1.100.0, Bun 1.4,
**no llama.cpp binaries installed**, no GUI libraries. That constrains what can be
proven here; this file separates what was run from what was not.

## Executed and passing

| Check | Command | Result |
| --- | --- | --- |
| Full workspace tests | `cargo test --workspace` | **81 passed, 0 failed** |
| GGUF parser (hooks, arrays, bounds, KV estimate) | `cargo test -p atom-models` | 12 passed |
| Path allowlist / traversal / checksum | `cargo test -p atom-security` | 4 passed |
| Quantize planning, argv, progress parsing, validation | `cargo test -p atom-compression` | 11 passed |
| Quantize runner against a stub `llama-quantize` process | `cargo test -p atom-compression --test stub_tool` | 5 passed |
| llama-server supervision + SSE streaming (stub process) | `cargo test -p atom-inference --test streaming` | 4 passed |
| Deterministic scoring + consistency | `cargo test -p atom-bench` | 6 passed |
| Engine jobs end-to-end (import, refusal, backend gate) | `cargo test -p atom-engine --test engine` | 6 passed |
| Engine end-to-end with both stub binaries | `cargo test -p atom-engine --test stub_backend` | 5 passed |
| Frontend typecheck | `npx tsc --noEmit` | clean |
| Frontend typecheck after UI changes | `npx tsc --noEmit` | clean |
| Frontend production build | `bun run build` | built, `frontend/dist` |
| API + built UI served | `curl :8787/`, `/api/health`, `/api/models` | 200 text/html, JSON OK |
| All routes render | Playwright over the dev preview, 1440×950 and mobile width | no page errors, no horizontal overflow |
| Micro API workflow | `.run/e2e_micro.py` against a live API | project → dataset → train → evaluate → int8 → memory → export → host bench → import → infer all succeeded (numbers below) |
| CLI | `atom status` | reports both binaries missing and ready=false |

### What the stub-driven tests actually prove

`crates/atom-teststubs` provides two dev-only executables. They are real
processes with real pipes, real exit codes and a real HTTP socket; ATOM's code
under test is unchanged.

`stub-llama-quantize`:

1. **Progress comes from the tool.** Every `[n/total]` line the stub prints is
   parsed and turned into job progress, and the raw line is kept in the job log.
2. **Cancellation.** With the stub hanging mid-run, `request_cancel` kills the
   child, deletes the partial output, leaves the source byte-identical and
   returns a `cancelled` error — never success.
3. **Failure.** A non-zero exit removes the partial file and reports the real
   exit code and the tool's last lines.
4. **Success without output**, and **empty output**, are both rejected.
5. **End-to-end engine run:** source imported → quantized → validated (readable
   GGUF, tensor count equal to source, smaller than source, checksum recorded) →
   indexed as a compressed variant linked to the source, with a run log written
   next to the output and the source checksum unchanged.
6. **Bad output is refused:** a tool that exits 0 but writes a non-GGUF file
   produces a failed job, no library entry, and no leftovers (output and run log
   are removed).

`stub-llama-server`:

7. **Measured load time.** The stub delays binding its socket; the reported
   `load_ms` reflects that delay and is bounded by wall-clock time.
8. **Health/props.** Startup waits for a real `/health` response and reads
   `build_info` from `/props`.
9. **Token streaming.** First-token latency, one delta per SSE chunk, usage
   parsed from the server (not estimated), and throughput computed from the
   measured window.
10. **Mid-stream cancellation** emits `Cancelled` with the token count actually
    received, promptly, and the engine keeps the partial answer marked with `…`
    while recording no throughput metrics.
11. **HTTP errors** surface as an error event to the UI and as an `Err` from the
    batch path — never as a silent empty answer.
12. **Unload** reaps the child (verified via `/proc/<pid>` disappearing).

### ATOM Micro, measured in this VM

| Operation | Observed result |
| --- | --- |
| Dataset import | 1 document, 10,976 bytes, entropy 4.24 bits/byte, split 9,331 / 1,646 chars |
| Training (`atom-mlp-tiny`, 6 epochs, 3,000 windows/epoch) | loss 4.3490 → validation 0.7096, perplexity 2.03, top-1 92.1%, 6.55 s |
| Evaluation | 1,640 held-out windows, perplexity 2.017 |
| int8 quantization | 45,248 weight bytes, 3.83× smaller than f32, loss delta +0.0006 |
| RAM/flash plan (ESP32-S3) | fits: 42.20 KiB RAM (8%), 380 KiB flash (4.6%) |
| Export | 7 files, 194,156 bytes, SHA-256 manifest |
| Host compile check | `cc` compiled the exported runtime and measured 210,084 tok/s (1-byte MLP, host reference) |
| Device report import | 1 run accepted (176,678 tok/s, 0.01 ms, 46.27 KiB); malformed or zero numbers refused |
| Inference | 64-byte completion measured in 7 ms |
| Missing PlatformIO | build endpoint returns HTTP 503 `unavailable` before starting a job |
| Missing teacher | distillation returns HTTP 503 with the exact requirement |

Training loss, perplexity and quantization deltas are computed from the model; they
are not constants in the response. The host throughput reflects a tiny model on
this VM, not a microcontroller.

## ATOM Micro not executed here

1. **Firmware build and flash.** No PlatformIO Core (`pio`) and no serial board.
   The generated project is complete, but `pio run`/upload were not run; the API
   refuses with 503 instead of pretending.
2. **Transformer Micro architectures.** Listed and parameter-counted, but
   training requires PyTorch, which is absent.
3. **Distillation.** Needs a loaded `llama-server` teacher; no GGUF/backend here.
4. **Real board benchmarks.** The imported report came from the exported host
   harness. Real ESP32/RP2040 numbers require flashing a board.

## Not executed here

These require artifacts this environment does not have. They are implemented but
**not empirically verified end-to-end in this VM**:

1. **Real llama.cpp conversions and inference.** The stubs prove ATOM's own
   supervision, parsing, cleanup and streaming behaviour; they cannot prove that
   a given llama.cpp build accepts the exact argv or produces the tensor layout
   ATOM expects. No real multi-GB conversion or real model inference was run.
2. **Real comparisons.** No model, no backend → no measured pass rates.
3. **Desktop shell.** `apps/desktop` needs WebKitGTK/GTK (Linux) or Xcode/WebView2;
   absent here. It is excluded from the Cargo workspace for that reason.
4. **A real GGUF model.** All model metadata tests use a synthetic file. Unknown
   architectures fall back to `llama.*` metadata keys and report missing fields as
   `—` rather than guessing.

## How to verify on a machine with llama.cpp

```bash
# toolchain
llama-server --version
llama-quantize --help | head -40

cargo build --release
ATOM_HOME=/tmp/atom-verify ./target/release/atom-api

# in another shell
./target/release/atom status
./target/release/atom inspect /path/to/model-f16.gguf
./target/release/atom import /path/to/model-f16.gguf
./target/release/atom formats <model-id>          # formats advertised by *your* binary
./target/release/atom quantize <model-id> Q4_K_M  # progress + validation output
```

Then, in the UI:

1. **Library** — metadata, tensor mix, checksum; run *Verify checksum* twice and
   confirm the second run re-hashes the same file.
2. **Inference** — load with mmap on and off; confirm load time, TTFT and tok/s
   update and that **System Monitor** shows llama-server RSS changing while
   system RAM moves independently.
3. **Compression Studio** — confirm the format list matches
   `llama-quantize --help`, run Q4_K_M, watch per-tensor progress, cancel mid-run
   and confirm the partial file is gone, then run to completion and confirm the
   validation checklist and the source checksum unchanged.
4. **Comparison Lab** — compare original vs. compressed with fixed seed; confirm
   disk saved, peak RSS, tok/s and suite pass rates are populated and the Markdown
   report downloads with prompt outputs.
5. **Source protection** — `sha256sum` the original before and after the run; it
   must be identical.

## How to verify ATOM Micro on a machine with PlatformIO

```bash
pip install platformio
cargo build --release
ATOM_HOME=/tmp/atom-micro ./target/release/atom-api
```

Create a project in **Micro Studio**, import a UTF-8 text dataset, train
`Atom MLP · Tiny`, evaluate, quantize to int8, analyse memory, export (watch the
host compile check), then **Build** — `pio run` output streams into the job log.
Attach the board, flash, open the serial monitor and send `bench`, then import the
JSON it prints in **Benchmark Center**.

## Honesty checks in the product

- With no backend, the UI shows a "llama.cpp not fully detected" banner, disables
  run buttons and explains what is missing.
- Live tuning is described as runtime configuration; only the Compression Studio
  is described as permanent.
- Quality is labelled "automated estimate" in the comparison cards, the report
  header and the README.
- The Settings network switch states plainly that no download path exists.

## How to verify the desktop build

The desktop shell is verified by building its installer on the target OS and
opening the app; there is nothing to verify in a browser.

```bash
# Linux
scripts/build-desktop.sh
sudo apt install ./apps/desktop/target/release/bundle/deb/ATOM_0.2.1_amd64.deb
atom   # or launch ATOM from the application menu
```

```powershell
# Windows
powershell -ExecutionPolicy Bypass -File scripts\build-desktop.ps1
# then run the installer from apps\desktop\target\release\bundle\nsis
```

Checks that matter:

1. The window opens with the ATOM dashboard and the title bar icon, with **no
   browser and no terminal** running.
2. `curl http://127.0.0.1:<port>` is irrelevant — the port is ephemeral — but the
   System Monitor page must show live samples, so the in-process API is serving.
3. Unplug the network: the app keeps working (everything is local).
4. Close the window: the process exits and nothing stays listening
   (`ss -ltnp | grep atom`).
5. The install directory contains one executable and its icons; the app starts
   without `frontend/dist` anywhere on disk, because the UI is embedded.

This Linux sandbox has no root and no GUI libraries, so the artifacts themselves
were not produced here. What *was* verified here: the `embed-ui` feature builds,
an `atom-api` binary compiled with it serves the interface (HTML, hashed assets
with correct content types, SPA deep-link fallback, `/api` routes) after
`frontend/dist` is moved away, and the full workspace test suite still passes.


## 0.2 update — Windows startup fix

The desktop shell created its webview synchronously inside Tauri's `setup`
handler. Tauri documents that this **deadlocks on Windows** (wry#583): the
window never appears, while the same code opens normally on Linux. The shell now
builds the window on a worker thread on Windows, keeps the synchronous path on
Linux (GTK requires the main thread), and gives the webview an explicit writable
data directory (`<ATOM_HOME>/webview`) so WebView2 cannot fail on a missing
profile folder.

Failures are no longer silent on Windows: startup errors are written to
`%USERPROFILE%\.atom\logs\desktop.log`, shown in a native `MessageBoxW` dialog,
and the panic hook reports crashes the same way. If the WebView2 runtime is not
detected under `Program Files`/`LOCALAPPDATA`, the error adds the Evergreen
Runtime download link. The NSIS/MSI installers now embed the WebView2
bootstrapper instead of downloading it during installation.

| Check | How | Status |
| --- | --- | --- |
| Windows compile and installer | `.github/workflows/desktop.yml` on `windows-latest` | see the workflow run linked in the commit/PR |
| Windows window appears | launch the installer on a real Windows 10/11 machine | **not run here** — no Windows host or display in this environment |
| Linux desktop build | `scripts/build-desktop.sh` | not re-run (no WebKitGTK/GTK headers here) |

## 0.2.1 update — startup crash after the window fix

The 0.2.0 Windows installer opened the window and then stopped with
"ATOM stopped unexpectedly … panicked at crates/atom-engine/src/state.rs:229:
there is no reactor running, must be called from the context of a Tokio 1.x
runtime".

`AppState::spawn_sampler` used a bare `tokio::spawn`, and the desktop shell calls
it from `main` before Tauri has started its Tokio runtime, so the sampler panicked
on every platform. The sampler now spawns onto the ambient runtime when there is
one (`Handle::try_current`) and otherwise runs on its own single-threaded runtime
thread, so it no longer depends on the caller's runtime or call order. A
regression test (`crates/atom-engine/tests/sampler.rs`) calls the sampler from a
plain `#[test]`, i.e. with no runtime entered.

| Check | Command | Result |
| --- | --- | --- |
| Sampler starts without a runtime | `cargo test -p atom-engine --test sampler` | passes |
| Engine builds | `cargo check -p atom-engine` | clean |
| Windows window + sampler | launch `ATOM_0.2.1_x64-setup.exe` on Windows 10/11 | **not run here** — no Windows host or display in this environment |

## 0.2 update — classification and UI fixes

Environment unchanged: Linux cloud VM, no llama.cpp, no PlatformIO, no board, no
display. Commands below were run in this session.

| Check | Command | Result |
| --- | --- | --- |
| Workspace tests | `cargo test --workspace` | **89 passed, 0 failed** (7 new classifier tests included) |
| Classifier unit tests | `cargo test -p atom-micro --lib` | 36 passed, 7 new |
| Split assignment invariant | `cargo test -p atom-micro split_is_assigned_once` | passes: membership and index vectors come from one assignment; every row appears exactly once; duplicates share a split; a rebuild reproduces the same split |
| Frontend typecheck | `./node_modules/.bin/tsc --noEmit` | clean |
| Frontend build | `bun run build` | built, 998 kB JS / 47 kB CSS |
| API build | `cargo build --release -p atom-api` | succeeded |
| CSV profile | `POST /micro/projects/{id}/classification/profile` | 600 rows; `wind_kmh` numeric with 7 invalid cells, `humidity_pct` 12 missing, `temperature_c` 15 missing |
| Dataset import | `POST …/classification/dataset` | 12 design features, 3 classes (186 / 166 / 248), split 420 / 90 / 90, 27 imputed missing values, 0 duplicate rows |
| Training job | `POST …/classification/train` | succeeded; 80 epochs in 9 ms; 39 parameters; test accuracy **0.8778**, macro F1 0.8784, weighted F1 0.8804, log loss 0.3877 |
| Confusion matrix | job result | `[[29,0,5],[0,20,5],[0,1,30]]` on 90 held-out rows |
| Classifier export | `POST …/classification/export` | 4 files + manifest; JSON reloaded and re-scored 90 rows at the same accuracy; `cc` compiled the self-test which printed `predicted=heating actual=heating classes=3 features=12` |
| Duplicate GGUF import | `POST /models` twice with the same file | second job failed: "already in the library … duplicate copy was discarded"; models directory unchanged |
| Malformed GGUF | `POST /models` with a 12-byte non-GGUF file | failed: "not a GGUF file (missing GGUF magic)" |
| Path allowlist | `POST /models` with `/etc/passwd` | failed: "outside the approved model and workspace directories" |
| System Monitor axis labels | Playwright, 1440 px | ticks render `0 B … 572 MiB` and `0% … 100%`; no clipped `1%` / `00M` |
| Library drop zone | Playwright, 1440 px | text is "Drop a GGUF model here or browse files"; no absolute workspace path in the hint |
| Classification UI | Playwright, `/micro/<id>/classifier` | profile table, class distribution, 87.78 % accuracy, macro/weighted scores, loss curve and confusion matrix all render; export panel shows the bundle |

### Still blocked in this VM

- Real `llama-quantize` / `llama-server` runs: neither binary is installed. The
  stub-driven tests remain the only end-to-end evidence for those paths.
- PlatformIO firmware build/flash and device benchmarks: no `pio`, no board.
- Native desktop window: no WebKitGTK/GTK headers and no display.
- A real user CSV: the classification numbers above come from the synthetic
  fixture (`scripts/make_classification_fixture.py`), which the README labels.
