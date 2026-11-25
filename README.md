# ATOM — Adaptive Tiny Operating Model

A local-first desktop application for importing GGUF language models, tuning how
they run, **permanently** compressing them into separate files, comparing
original vs. compressed behaviour with measured numbers, and training/exporting
tiny specialist models for microcontrollers with **ATOM Micro**.

Built in Rust (Tauri shell, axum API, llama.cpp backend) with a React +
TypeScript + Tailwind interface.

> ATOM is a developer tool, not a chatbot demo. Every metric shown is either
> measured from the OS or the tool's own output, or clearly labelled as an
> estimate. When llama.cpp is missing, ATOM says so and disables the feature
> instead of simulating it.

---

## Two optimization modes, deliberately separate

| | Live runtime optimization | Permanent compression |
| --- | --- | --- |
| What changes | How llama.cpp loads and runs the model | A new GGUF file on disk |
| Reversible | Yes — unload/reload | No, the output is a new artifact |
| Knobs | context, threads, batch, mmap, mlock, flash-attn, GPU layers, KV-cache precision, memory budget | llama-quantize type (Q8_0 … Q2_K) |
| Original file | Never modified | Never modified, never deleted |
| Evidence | load time, RSS, TTFT, tok/s | size delta, checksums, validation report |

ATOM does **not** claim dynamic weight compression, layer skipping or
"intelligent" sparsity. Those are not operations llama.cpp exposes here, so they
are not offered.

---

## What is implemented

- **Model library** — GGUF v1–v3 header/metadata/tensor parser written in this
  repo, streaming SHA-256, import by path or upload, drag-and-drop, filter/search,
  variant links (compressed → source), verification and tamper detection.
- **Permanent compression** — detects the installed `llama-quantize`, reads the
  quantization types *its own help output advertises*, computes real per-tensor
  size estimates, runs it with an explicit argv, parses its `[n/total]` tensor
  progress, supports cancellation, deletes partial output, then re-parses and
  validates the result before it enters the library.
- **Inference** — supervises `llama-server` (HTTP), streams tokens, cancels
  immediately, measures TTFT/throughput/load time, and samples the child
  process's real RSS.
- **Comparison Lab** — identical prompts and runtime settings across 1–3 models;
  disk, throughput, latency, peak RSS, per-suite pass rates, repeat-consistency;
  Markdown + JSON reports; history.
- **System monitor** — sysinfo-backed samples streamed over SSE, with system RAM,
  ATOM process RSS and model process RSS kept visibly separate.
- **Quality suites** — deterministic, rule-checked cases for instruction
  following, knowledge, reasoning, arithmetic, coding and consistency. Results
  are labelled automated estimates, never "intelligence scores".
- **ATOM Micro** — a complete 11-step workflow (project → board → dataset →
  architecture → training → evaluation → quantization → memory plan → export →
  firmware → device benchmarks) backed by the `atom-micro` crate. Byte-level MLP
  specialists train in-process with real SGD; exported int8/int4 artifacts ship
  with a C runtime, a host harness that is compiled and measured by ATOM, and a
  generated PlatformIO project for the selected board.
- **Desktop shell** — Tauri 2 app that hosts the same API on a loopback port and
  loads the same UI.
- **CLI** — `atom status | inspect | models | formats | import | quantize`.

## ATOM Micro

ATOM Micro is a separate, self-contained pipeline for specialists that fit on a
microcontroller. It is not a wrapper around a cloud API and it does not fake a
training run.

| Step | Operation | What is real |
| --- | --- | --- |
| 1–2 | Project and target board | project persisted on disk; board limits from `atom-micro` hardware tables |
| 3 | Dataset import | file copied into the ATOM home, document/byte stats, deterministic train/val split |
| 4 | Architecture | 3 built-in MLP specs (parameters computed from the spec), 2 transformer specs that require PyTorch |
| 5 | Training | real byte-level MLP, in-process SGD, per-epoch train/validation loss, cancellable job |
| 6 | Evaluation | measured validation windows, perplexity and next-byte accuracy |
| 7 | Quantize | symmetric per-row int8/int4 with measured size ratio and loss delta |
| 8 | RAM / flash | weight bytes plus activation/heap estimate against the board's datasheet limits (labelled estimate) |
| 9 | Export | `model.bin`, `model.c/h`, `runtime.c`, `main.c`, manifest with SHA-256, README, then a host compile + benchmark check |
| 10 | Firmware | generated PlatformIO project, `pio run`, upload and serial monitor; requires PlatformIO |
| 11 | Benchmarks | import a device JSON report or read serial `bench` output; fabricated/zero numbers are rejected |

Architectures that need PyTorch (transformers) are listed but disabled with the
exact requirement. Firmware build, flashing and teacher-model distillation report
an unavailable state when PlatformIO, a serial port or a loaded `llama-server`
teacher are missing — they never simulate success.

## Requires an external llama.cpp build

ATOM does **not** ship or download inference binaries.

- `llama-server` → inference, chat, benchmarking
- `llama-quantize` → permanent compression

Set explicit paths in **Settings**, or put them on `PATH`. Both the API and the
UI report precisely which of the two is missing and what that disables.

## Quick start

```bash
# 1. Backend + API
cargo build --release
ATOM_HOME="$HOME/.atom" ./target/release/atom-api      # http://127.0.0.1:8787

# 2. UI for development (proxies /api to the API)
cd frontend
bun install
bun run dev                                            # http://127.0.0.1:3003

# 3. Production UI, served by the API
bun run build                                          # writes frontend/dist
```

## Desktop app (Windows + Linux)

ATOM runs as a native desktop application: the Rust core, the API and the
compiled interface are embedded in one executable, the window is served from an
ephemeral loopback port, and no browser, terminal or `frontend/dist` folder is
needed on the target machine.

```bash
# Linux: .deb + AppImage (needs WebKitGTK 4.1 dev packages)
scripts/build-desktop.sh

# Windows: NSIS .exe + .msi (run in PowerShell, needs Rust MSVC + WebView2)
scripts\build-desktop.ps1
```

Installers land in `apps/desktop/target/release/bundle/`. Pushing a `v*` tag runs
[.github/workflows/desktop.yml](.github/workflows/desktop.yml), which builds and
attaches both platforms' installers to the GitHub release. Full prerequisites and
data locations: [apps/desktop/README.md](apps/desktop/README.md).

CLI:

```bash
./target/release/atom status
./target/release/atom inspect ~/Models/model-f16.gguf
./target/release/atom import ~/Models/model-f16.gguf
./target/release/atom formats <model-id>
./target/release/atom quantize <model-id> Q4_K_M
```

## Repository layout

```
crates/
  atom-core/        config, errors, job registry, log buffer
  atom-security/    path allowlist, checksums, input validation, argv rules
  atom-models/      GGUF parser, quant catalog, library, import, verification
  atom-memory/      sysinfo sampling (system + per-process)
  atom-inference/   llama-server supervision, streaming, tool probing
  atom-compression/ quantize planning, execution, output validation
  atom-bench/       deterministic suites, scoring, comparison store/reports
  atom-micro/       boards, datasets, MLP model, quantization, export, firmware, benchmarks
  atom-engine/      composition root used by API, CLI and desktop
apps/
  api/              axum HTTP + SSE surface, serves the built UI
  cli/              `atom` command
  desktop/          Tauri 2 shell: embeds the UI, Windows/Linux bundle config, icons
frontend/
  src/components/   design system, charts, job console, comparison view
  src/pages/        Onboarding, Overview, Library, Compression, RuntimeOptimization,
                    Comparison, Inference, Benchmarks, MicroProjects, MicroStudio,
                    Training, Distillation, Hardware, Deployment, Monitor, Settings
docs/               architecture and verification notes
```

## Safety properties enforced in code

- Every path is canonicalized and checked against an allowlist of roots
  (`PathGuard`); traversal and symlink escapes are rejected.
- External tools are spawned with explicit argv — never through a shell.
- Output paths are refused when they equal the source path; source files are
  never deleted by ATOM.
- Partial or invalid quantization output is deleted, never presented as success.
- Job cancellation kills the child process and reaps it.
- Model metadata is parsed as data only; nothing from a model file is executed.
- Network access is off by default, and no download code path exists yet.

## Tests

```bash
cargo test --workspace     # 81 tests: parser, security, quantize, scoring, engine jobs,
                           # Micro training/export/quantization, plus stub-driven
                           # process/streaming integration tests
cd frontend && npx tsc --noEmit && bun run build
```

`crates/atom-teststubs` holds two dev-only executables that impersonate
`llama-quantize` and `llama-server`. The integration tests spawn them as real
child processes, so cancellation cleanup, output validation, measured load time
and SSE token streaming are exercised without a llama.cpp build or a real model.

See [docs/VERIFICATION.md](docs/VERIFICATION.md) for exactly what was verified in
the build environment and what was not.

## Known limitations

- **No llama.cpp in this build environment.** Process supervision, tensor
  progress parsing, cancellation cleanup, output validation and SSE streaming
  are verified against stub executables (`crates/atom-teststubs`), but no real
  multi-GB conversion or real model inference was performed here. The UI still
  reports a missing backend as unavailable rather than simulating it.
- **Desktop installers are not built in this environment.** The Tauri shell,
  embedded-UI serving and bundle configuration are complete, but this Linux
  sandbox cannot install WebKitGTK/GTK development packages (no root), so the
  `.deb`/AppImage/MSI/NSIS artifacts must be produced by the build scripts above
  or by the desktop workflow.
- Model downloads are **not implemented**; the Settings toggle only records
  intent for a future opt-in, checksum-verified flow.
- **ATOM Micro firmware steps need PlatformIO** (`pip install platformio`). The
  export step still compiles and benchmarks on the host with `cc`, but building
  and flashing board firmware is unavailable without `pio`.
- **Transformer Micro architectures need PyTorch** (`pip install torch`). The
  built-in MLP trainers do not.
- Distillation uses a loaded `llama-server` teacher; without one the lab is
  disabled and explains what to load.
- Device benchmark numbers are imported from a real board (or the exported host
  harness); ATOM never invents them.
- GPU offload is passed through as `-ngl`; ATOM does not probe GPU memory.
- Quality suites are small and rule-based by design. They detect regressions on
  those specific behaviours, not general model equivalence.
- `mcp`/agent features (`atom-agent`) are a future phase and absent.
