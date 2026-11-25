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
sudo apt install ./apps/desktop/target/release/bundle/deb/ATOM_0.1.0_amd64.deb
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
