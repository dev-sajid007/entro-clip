# EntroCut

A desktop video editor built with Rust, Tauri 2, React, TypeScript, and FFmpeg.

See [EntroCut-Project-Documentation.md](EntroCut-Project-Documentation.md) for the architecture and roadmap.

## Prerequisites

- Rust (stable), Node.js 24+, pnpm
- FFmpeg and FFprobe on `PATH`
- Tauri system dependencies for your OS: https://v2.tauri.app/start/prerequisites/

## Development

```bash
pnpm install
pnpm tauri dev      # run the desktop app
pnpm test           # frontend tests (Vitest)
pnpm test:rust      # backend tests
```

Set `RUST_LOG` to change backend log verbosity, e.g. `RUST_LOG=debug pnpm tauri dev`.
