# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

EntroCut is a desktop video editor: a Tauri 2 shell around a React 19 + TypeScript frontend (`src/`) and a Rust backend (`src-tauri/`, crate `entrocut_lib`). It shells out to FFmpeg/FFprobe, which must be on `PATH`. `EntroCut-Project-Documentation.md` is the design spec and milestone roadmap (§17). Work is delivered milestone by milestone, and commits are named after them (e.g. "Milestone 4, part 1"). Read the relevant spec section before starting a milestone.

## Commands

```bash
pnpm install
pnpm tauri dev                 # run the desktop app (starts Vite on :1420)
pnpm build                     # tsc type-check + Vite build
pnpm test                      # frontend tests (Vitest)
pnpm test:rust                 # backend tests (cargo test)

pnpm vitest run src/lib/timeline.test.ts         # one frontend test file
pnpm vitest run -t "name substring"              # frontend tests by name
cargo test --manifest-path src-tauri/Cargo.toml undo_and_redo   # Rust tests by name filter
cargo clippy --manifest-path src-tauri/Cargo.toml
```

Backend logging uses `tracing`. The default filter is `info,entrocut_lib=debug`, and `RUST_LOG` overrides it.

## Architecture

**The backend owns the canonical project.** `state.rs` holds `AppState`, which contains a `Mutex<Session>`. A `Session` has the `Project`, its save path, and the undo/redo stacks of whole-project snapshots. Dirty tracking compares `state_id` with `saved_state_id`, so undoing back to the saved state makes the project clean again. The frontend never mutates project data. It receives `ProjectState` snapshots (project + path + dirty + canUndo/canRedo + missingAssetIds) and replaces its copy with each one.

**Edits are semantic commands.** The frontend sends an `Edit` enum value (`domain/edit.rs`) through the `apply_edit` command. `edit::apply` works on a *clone* of the project, then validates it (`Project::validate` enforces the invariants in spec §6.4). `Session::commit` runs only if validation passes, so a rejected edit leaves state untouched. Any new mutation should be a new `Edit` variant, not a new ad-hoc command.

**Layering in `src-tauri/src`:**
- `domain/`: pure types and logic with no Tauri or process dependencies, unit-tested in place (`#[cfg(test)]` modules). Includes `project.rs` (schema and validation, `SCHEMA_VERSION`), `edit.rs`, `time.rs`, `render_plan.rs`, and `errors.rs`.
- `services/`: everything that touches the filesystem or FFmpeg (probe, thumbnails, preview frames, project store, render). All tool invocations go through `services::tools::run_tool`, which takes an argument array and never uses a shell string.
- `commands/`: thin `#[tauri::command]` wrappers. Register each new command in `lib.rs` `generate_handler!` **and** add a typed wrapper in `src/lib/tauri.ts`.
- `jobs/`: `JobManager` (one export at a time, cancel via `RenderControl`) and event names/payloads. `commands/export.rs` runs the render on its own thread and reports through `export-progress` / `export-completed` / `job-failed` events, with progress emits throttled. Cancellation arrives as `job-failed` with kind `cancelled`. The frontend calls `get_job_status` after `start_export` returns to catch up on events sent before it knew the job id.

**Concurrency rules:** never hold the `session()` guard across an `.await` or a blocking FFmpeg call. Async commands take a short lock to copy what they need, run the media work in `spawn_blocking`, and re-lock to commit (see `commands/media.rs`). Semaphores in `AppState` limit thumbnail and preview concurrency. Preview requests carry a `preview_generation` counter: a superseded request returns `AppError::Cancelled`, and the frontend ignores that error.

**Errors across IPC:** `AppError` serializes as `{ kind, message }`, where `kind` is a camelCase string from `AppError::kind()`. When adding a variant, also add its `kind` arm. The TS mirror is the `AppError` interface in `src/lib/tauri.ts`.

**Serialization contract:** Rust structs use `#[serde(rename_all = "camelCase")]`, and tagged enums use `tag = "status"`/`"state"`/etc. The TS types in `src/lib/tauri.ts` mirror them by hand, so any change to a serialized Rust type must be matched there.

**Time:** canonical time is integer microseconds (`*_us` in Rust, `*Us` in TS, type `Microseconds`). Frame rates are `Rational`. `domain/time.rs` converts to and from FFmpeg's decimal-second strings without going through floats. On the frontend, all pixel↔time conversion goes through `usToPx`/`pxToUs` in `src/lib/timeline.ts`.

**Preview vs export:** a paused preview shows an exact FFmpeg-extracted frame of the top-most visible video clip, cached on disk under `$APPCACHE/frames` and served through Tauri's asset protocol (scope in `tauri.conf.json`). During playback, `usePlayback` runs a wall-clock clock that `<video>`/`<audio>` elements follow. Only the top visual clip is shown, with no overlay compositing. Export goes Project → typed `RenderPlan` (`domain/render_plan.rs`, pure and tested without FFmpeg) → `ffmpeg_args`/`filter_graph` → `services/render.rs`, which renders to a hidden `.partial.mp4` sibling, probes it, then renames it. The H.264 encoder is detected at runtime (`render::h264_encoder`): it prefers `libx264` and falls back to `libopenh264` (bitrate-based), because Fedora's `ffmpeg-free` lacks x264. Don't hardcode `libx264`. Preview and render plan must agree on stacking rules: the first video track is on top, video clips carry their own audio, and muted tracks are skipped. `preview::fit_filter` keeps the letterboxing consistent between them.

**Persistence:** `services/project_store.rs` writes atomically (temp file + fsync + rename), caps file size before parsing, and runs `migrate` then `validate` on load. Bump `SCHEMA_VERSION` and add a migration step when the on-disk format changes. A missing source file is a warning (`missingAssetIds`), never a load failure.

**Frontend layout:** `src/lib/` holds pure, Vitest-tested helpers (time, timeline geometry, playback clip resolution) plus the IPC bridge. `src/features/` holds hooks (`useProject` owns project state, dialogs, and the unsaved-changes prompt; `usePlayback` owns the clock). `src/components/` holds the four editor panels (media library, preview, timeline, inspector), composed in `src/app/App.tsx`.
