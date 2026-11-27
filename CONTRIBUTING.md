# Contributing to ATOM

Thanks for taking a look. ATOM is a small Rust + Tauri + React desktop app for
running and benchmarking GGUF models locally. Contributions are welcome — bug
reports, docs fixes, and focused features.

## Before you start

- Open an issue first for anything larger than a one-line fix, so we can agree
  on the shape of the change.
- Keep pull requests focused. One change per PR is much easier to review than a
  bundle.
- Do not add telemetry, analytics, or network calls that send user data. ATOM
  is local-first; if a feature genuinely needs the network it must be opt-in
  and documented (see the "Model downloads" note in README).

## Build

```bash
# UI (the API embeds this build)
cd frontend && bun install && bun run build && cd ..

# Rust workspace (portable core)
cargo build --workspace

# Desktop shell (needs the platform GUI toolchain: webkit2gtk on Linux,
# MSVC + WebView2 on Windows)
cd apps/desktop && cargo build
```

## Tests

```bash
cargo test --workspace          # Rust unit/integration tests
cd frontend && bun run typecheck
```

The desktop shell and the native window are not exercised in CI's Linux image
beyond compilation; see `docs/VERIFICATION.md` for what is and is not covered.

## Pull requests

- Run the tests above before pushing.
- The `Desktop installers` workflow runs on pull requests to `main` and on
  version tags; a green run is expected for merge.
- Match the existing code style. Rust is formatted with `rustfmt`; TypeScript
  follows the project's existing conventions (no new lint/format tools please).
- Commit messages: short imperative subject, optional body explaining *why*.
- Sign off is not required, but please use your real name in the author field.

## Reporting issues

Use the bug report template and include the ATOM version (shown in the sidebar
as `vX.Y.Z`), your OS, and the exact steps to reproduce. For crashes, attach
the log from `~/.atom/logs/` if it exists.

## License

By contributing you agree that your contributions are licensed under the
project's MIT license (see `LICENSE`).
