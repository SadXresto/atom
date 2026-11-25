# ATOM architecture

## Shape

```
React UI ──HTTP/SSE──> atom-api (axum) ──> atom-engine ──> crates/*
        └── same origin ──> Tauri window (desktop)
```

The desktop shell starts the same axum router on an ephemeral loopback port and
points the window at it. There is exactly one implementation of every operation,
one API contract and one UI.

`atom-engine::AppState` is the composition root: configuration, model library,
job registry, system monitor, the single live llama-server process, benchmark
store and conversations. `apps/api` contains only transport code.

## Crates

| Crate | Responsibility | Notable invariants |
| --- | --- | --- |
| `atom-core` | config, `Paths`, `AtomError`, `JobStore`/`JobHandle`, log buffer | errors carry stable codes + HTTP status; progress never moves backwards |
| `atom-security` | `PathGuard`, `sha256_file`, filename validation, executable lookup | no shell; canonicalize then containment check; file names reject separators/control chars |
| `atom-models` | GGUF parser, tensor type table, quant catalog, library, import, verification | header-only parse (never reads tensor payloads); sizes bounded; library writes are atomic (tmp + rename) |
| `atom-memory` | sysinfo sampling | reports system, own process and model process separately; no derived "total" |
| `atom-inference` | tool probing, `Runtime` (llama-server child), SSE parsing, `complete_at` | waits on `/health`; load time measured; child reaped on unload; cancellation checked per chunk |
| `atom-compression` | quantize tooling, plan/argv, runner, validation | source ≠ output enforced; progress parsed from tool output; partial output deleted on failure/cancel |
| `atom-bench` | suites, scoring, comparison types, store, Markdown report | every check is deterministic; consistency is agreement between repeats |
| `atom-engine` | orchestration of jobs, runtime, chat, comparisons | long work runs in tokio tasks; blocking work uses `spawn_blocking` |
| `atom-api` | routes, SSE, static UI hosting | destructive model deletion requires `confirm=<id>` |

## Long-running work

`JobStore` gives every job an id, a status, a progress fraction and an ordered
log. `JobHandle` writes to the registry *and* a broadcast bus. The UI subscribes
to `GET /api/jobs/{id}/stream` (SSE) and can also replay `GET /api/jobs/{id}/logs`.

Progress sources are real:

- **Import** — bytes copied (then checksum phase).
- **Quantize** — `llama-quantize` tensor counter `[n/total]` mapped to 5–95%,
  then validation.
- **Comparison** — one unit per benchmark case plus consistency repeats.

Nothing else exists that could claim progress, so nothing else does.

## Inference lifecycle

1. `POST /api/runtime/load` validates the record, computes a memory estimate from
   GGUF metadata, and spawns `llama-server` with an explicit argv.
2. Startup is considered complete only when `/health` answers 200; the child is
   watched for early exit and a 180 s deadline.
3. Chat posts to `/v1/chat/completions` with `stream: true` and
   `stream_options.include_usage`. Deltas are forwarded as SSE; the first delta
   timestamps TTFT; `usage` supplies the token count. If a build omits usage the
   response is flagged `tokens_estimated`.
4. `POST /api/runtime/unload` kills and reaps the child; the monitor then reports
   no model process. `POST /api/runtime/release` additionally clears sample
   buffers and peak counters.

## Compression lifecycle

1. `GET /api/models/{id}/quantize/options` returns every catalog format annotated
   with support from three independent facts: does the installed binary advertise
   it, can this source be quantized (F32/F16/BF16, or Q8_0 with
   `--allow-requantize`), and is the estimated output smaller.
2. `POST /api/models/{id}/quantize` re-validates server-side, resolves the output
   inside the compressed-models root, refuses source==output, builds the argv.
3. The runner streams stdout/stderr into job logs, parses tensor progress, and on
   cancel or non-zero exit deletes the partial file.
4. Validation re-parses the output, compares tensor count, checks that it is
   smaller, hashes it, and inspects the dominant tensor type. A failed required
   check removes the file.
5. A `ModelRecord` with `kind = compressed { source_model_id, format, job_id, … }`
   is inserted, so provenance survives restarts.

## Benchmark lifecycle

`POST /api/comparisons` loads each model in turn with one shared `RunSettings`,
runs each selected case non-streaming, scores the output mechanically, samples
peak RSS every 300 ms, measures consistency by repeating one prompt, unloads, and
stores `{settings, environment, runs[], cases[]}` as JSON plus a Markdown report.

The quality index is the mean pass rate across quality suites. The UI labels it
as an automated estimate everywhere it appears.

## Frontend

- Tailwind v4 with a token-based dark theme (`@theme` in `src/index.css`).
- `useApi` for polling, `useLiveSystem` for the SSE monitor, `useJobStream` for
  job events with a polling fallback so a dropped stream cannot leave a spinner.
- Pages are thin: all numbers come from the API, all state that matters lives in
  the Rust core.
- Command palette (⌘K) plus `g`-prefixed navigation shortcuts.

## Deliberate non-goals (current phase)

- No model downloads.
- No GPU memory probing or automatic offload decisions.
- No claims about "intelligence"; only rule-checked behaviour on fixed prompts.
- No agent/system access (`atom-agent`).
