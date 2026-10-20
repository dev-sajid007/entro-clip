# EntroCut

A desktop video editor built with Rust, Tauri 2, React, TypeScript, and FFmpeg.

See [EntroCut-Project-Documentation.md](EntroCut-Project-Documentation.md) for the architecture and roadmap.

## Prerequisites

- Rust (stable), Node.js 24+, pnpm
- FFmpeg and FFprobe on `PATH`
- Tauri system dependencies for your OS: https://v2.tauri.app/start/prerequisites/
- Linux live preview playback uses WebKitGTK/GStreamer. For common codecs
  install the GStreamer plugins, e.g. on Fedora `gstreamer1-plugin-libav`
  (or `gstreamer1-plugin-openh264` for H.264 only). Paused still frames are
  rendered by FFmpeg and do not depend on GStreamer.

## Preview limitations

- Paused: exact frames from FFmpeg, showing the top-most visible video track.
- Playing: the top-most visual clip plays live; overlays are not composited
  until export. Audio from all unmuted clips plays together.

## Development

```bash
pnpm install
pnpm tauri dev      # run the desktop app
pnpm test           # frontend tests (Vitest)
pnpm test:rust      # backend tests
```

Set `RUST_LOG` to change backend log verbosity, e.g. `RUST_LOG=debug pnpm tauri dev`.
