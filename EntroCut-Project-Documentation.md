# EntroCut --- Project Documentation

**A CapCut-inspired desktop video editor built with Rust, Tauri 2,
React, TypeScript, and FFmpeg**

-   **Project name:** EntroCut (working name; check trademarks before
    public release)
-   **Document version:** 1.0
-   **Status:** Architecture and implementation plan
-   **Initial target:** Linux desktop, with cross-platform architecture
-   **License:** To be decided before distribution

------------------------------------------------------------------------

## 1. Project Overview

EntroCut is a non-linear desktop video editor (NLE) designed around a
modern timeline workflow. It aims to provide the core editing experience
of consumer editors such as CapCut while keeping the application
modular, local-first, and extensible.

The first release should focus on a dependable editing loop:

1.  Create or open a project.
2.  Import video, audio, and image assets.
3.  Inspect media metadata and generate thumbnails.
4.  Place clips on a multi-track timeline.
5.  Trim and move clips.
6.  Preview the timeline.
7.  Export a rendered MP4 file.
8.  Save and reopen the editable project.

Advanced capabilities---transitions, keyframes, effects, captions,
templates, and AI-assisted editing---are planned after the MVP is
reliable.

### 1.1 Product goals

-   Provide a native desktop application for Linux, Windows, and macOS
    over time.
-   Use Rust for application services and correctness-sensitive
    operations.
-   Use React and TypeScript for a responsive, accessible editing
    interface.
-   Use FFmpeg as the initial media probing and rendering backend.
-   Keep project files portable and human-readable where practical.
-   Make editing non-destructive: edits change project metadata, not
    source media.
-   Support background rendering with progress, cancellation, and clear
    errors.

### 1.2 Non-goals for the MVP

-   Matching every CapCut feature.
-   Cloud collaboration or cloud project storage.
-   Real-time multi-user editing.
-   Building a codec implementation from scratch.
-   Guaranteed professional-grade color grading.
-   AI generation, automatic beat sync, or automatic scene editing in
    the first release.

------------------------------------------------------------------------

## 2. Recommended Technology Stack

  -----------------------------------------------------------------------
  Area                    Choice                  Responsibility
  ----------------------- ----------------------- -----------------------
  Desktop runtime         Tauri 2                 Desktop shell, native
                                                  integration, IPC

  Backend                 Rust, stable toolchain  Project logic,
                                                  validation, media
                                                  services, job
                                                  management

  Frontend                React + TypeScript +    Editor UI and
                          Vite                    interactions

  Styling                 CSS Modules or plain    Layout and component
                          CSS                     styles

  Media probing           FFprobe                 Duration, streams,
                                                  codecs, dimensions,
                                                  frame rate

  Media rendering         FFmpeg                  Export, trimming,
                                                  scaling, overlays,
                                                  audio mixing

  Serialization           Serde + serde_json      Project file parsing
                                                  and writing

  IDs                     UUID                    Stable IDs for
                                                  projects, tracks,
                                                  clips, and assets

  Logging                 tracing                 Structured logs and
                                                  diagnostics

  Error handling          anyhow for application  Clear errors
                          boundaries; thiserror   
                          for typed domain errors 

  Tests                   Rust unit/integration   Core and UI correctness
                          tests; Vitest and React 
                          Testing Library         

  Package manager         pnpm                    Frontend dependencies

  Version control         Git                     Source history and
                                                  collaboration
  -----------------------------------------------------------------------

**Important media architecture decision:** Start with FFmpeg as a
separate background process. Do not send every video frame through Tauri
IPC. IPC is suitable for commands, state changes, progress events, and
metadata---not high-frequency frame transport.

FFmpeg licensing and redistribution must be reviewed before distributing
installers. The license obligations depend on how FFmpeg is built and
which codecs/libraries are included. Do not assume every prebuilt FFmpeg
binary has identical licensing terms.

------------------------------------------------------------------------

## 3. High-Level Architecture

``` text
┌──────────────────────────────────────────────────────┐
│                    React UI                          │
│ Media Library │ Preview │ Timeline │ Inspector       │
└─────────────────────────┬────────────────────────────┘
                          │ Tauri IPC
┌─────────────────────────▼────────────────────────────┐
│                 Rust Application Core                │
│ Commands │ Project Model │ Validation │ Undo/Redo    │
│ Media Service │ Render Job Manager │ File I/O        │
└───────────────┬───────────────────────┬──────────────┘
                │                       │
       ┌────────▼────────┐     ┌────────▼────────┐
       │ FFprobe / Media │     │ FFmpeg Renderer │
       │ Metadata        │     │ Export Pipeline │
       └────────┬────────┘     └────────┬────────┘
                │                       │
┌───────────────▼───────────────────────▼──────────────┐
│ Local source files │ Project JSON │ Cache │ Exports  │
└──────────────────────────────────────────────────────┘
```

### 3.1 Responsibilities

**Frontend** - Render media library, timeline, preview controls, and
properties inspector. - Manage transient UI state such as selection,
hover, zoom, and drag interactions. - Submit semantic editing commands
to the backend or validated domain layer. - Display progress, warnings,
and errors.

**Rust core** - Own the canonical project state and validate
mutations. - Enforce timeline invariants. - Save/load project files
safely. - Invoke media tools without blocking the UI thread. - Track
render jobs and publish progress events. - Handle cancellation and
process cleanup.

**FFmpeg / FFprobe** - Probe supported input media. - Generate
thumbnails and proxy media where needed. - Render the timeline into a
final file. - Report progress and errors to the Rust job manager.

### 3.2 Core design rules

1.  The project model is the source of truth---not the visual timeline
    component.
2.  Source files remain unchanged.
3.  Use stable IDs rather than array indexes to identify clips and
    tracks.
4.  Store time internally as integer ticks or rational frame/time
    values, not floating-point seconds.
5.  Generate render commands from validated project data; never execute
    arbitrary user-provided shell strings.
6.  Keep long-running work off the UI thread.
7.  Use a versioned project schema and migrations when the schema
    changes.

------------------------------------------------------------------------

## 4. Repository Structure

A single desktop app can start in one repository. Keep the boundaries
clear without introducing a monorepo toolchain until it is needed.

``` text
entrocut/
├── README.md
├── LICENSE
├── CONTRIBUTING.md
├── SECURITY.md
├── package.json
├── pnpm-lock.yaml
├── tsconfig.json
├── vite.config.ts
├── index.html
├── docs/
│   ├── ARCHITECTURE.md
│   ├── PROJECT-FORMAT.md
│   ├── RENDERING.md
│   ├── ROADMAP.md
│   └── ADR/
├── src/
│   ├── app/
│   │   ├── App.tsx
│   │   └── routes.ts
│   ├── components/
│   │   ├── media-library/
│   │   ├── preview/
│   │   ├── timeline/
│   │   ├── inspector/
│   │   └── export-dialog/
│   ├── features/
│   │   ├── project/
│   │   ├── media-import/
│   │   ├── clip-editing/
│   │   └── export/
│   ├── lib/
│   │   ├── tauri.ts
│   │   └── time.ts
│   ├── state/
│   ├── styles/
│   └── main.tsx
└── src-tauri/
    ├── Cargo.toml
    ├── tauri.conf.json
    ├── capabilities/
    └── src/
        ├── lib.rs
        ├── main.rs
        ├── commands/
        ├── domain/
        │   ├── project.rs
        │   ├── timeline.rs
        │   ├── media.rs
        │   └── errors.rs
        ├── services/
        │   ├── media_probe.rs
        │   ├── thumbnail.rs
        │   ├── project_store.rs
        │   └── render.rs
        └── jobs/
            ├── manager.rs
            └── events.rs
```

Keep domain logic separate from Tauri command functions so that most
timeline rules can be tested without launching a desktop window.

------------------------------------------------------------------------

## 5. Functional Requirements

### 5.1 MVP requirements

  -----------------------------------------------------------------------
  ID                      Requirement             Acceptance criteria
  ----------------------- ----------------------- -----------------------
  FR-01                   Create project          A new project has a
                                                  name, dimensions, frame
                                                  rate, and empty
                                                  timeline

  FR-02                   Save/open project       A project can be saved,
                                                  closed, and reopened
                                                  without losing edits

  FR-03                   Import media            User can select local
                                                  video, audio, and image
                                                  files

  FR-04                   Probe media             Supported assets show
                                                  duration, dimensions,
                                                  frame rate, and stream
                                                  information where
                                                  available

  FR-05                   Generate thumbnails     Video clips show
                                                  thumbnails; failures
                                                  are reported without
                                                  crashing

  FR-06                   Multi-track timeline    User can create tracks
                                                  and place clips on them

  FR-07                   Move and trim clips     Clip placement and
                                                  source in/out points
                                                  can be changed

  FR-08                   Playback preview        User can seek and
                                                  preview a basic
                                                  timeline sequence

  FR-09                   Export                  User can export a
                                                  timeline to MP4 with a
                                                  chosen resolution and
                                                  frame rate

  FR-10                   Render progress         UI displays progress,
                                                  completion, failure,
                                                  and cancellation

  FR-11                   Undo/redo               Basic timeline
                                                  operations can be
                                                  undone and redone

  FR-12                   Validation              Invalid clips, missing
                                                  files, and unsupported
                                                  media produce useful
                                                  errors
  -----------------------------------------------------------------------

### 5.2 Later requirements

-   Transitions and compositing.
-   Text, titles, stickers, and image overlays.
-   Audio gain, fades, waveform display, and multiple audio tracks.
-   Keyframe animation.
-   Speed changes and reverse playback.
-   Proxy workflow for high-resolution media.
-   Automatic captions and subtitle styling.
-   Color adjustments and filters.
-   Templates and presets.
-   AI-assisted captions, silence removal, scene detection, and editing
    suggestions.

------------------------------------------------------------------------

## 6. Project and Timeline Data Model

A project describes the edit; it does not contain the media itself. The
initial format should be JSON, with a schema version.

### 6.1 Time representation

Do not use floating-point seconds as the canonical representation.
Floating-point rounding can create cumulative timing errors.

For the first implementation, use integer microseconds for media times
or use rational frame/time values with a documented conversion policy.
If the timeline is frame-based, represent frame rate as a rational
number such as `30000/1001`, not just `29.97`.

Example: - `1 second = 1,000,000 microseconds` -
`2.5 seconds = 2,500,000 microseconds`

When snapping to a frame, convert through the project frame-rate
rational.

### 6.2 Example project JSON

``` json
{
  "schemaVersion": 1,
  "id": "project-uuid",
  "name": "My First Edit",
  "settings": {
    "width": 1920,
    "height": 1080,
    "frameRate": { "numerator": 30, "denominator": 1 },
    "audioSampleRate": 48000
  },
  "assets": [
    {
      "id": "asset-uuid",
      "kind": "video",
      "path": "../media/intro.mp4",
      "durationUs": 12500000,
      "width": 1920,
      "height": 1080
    }
  ],
  "tracks": [
    {
      "id": "track-uuid",
      "name": "Video 1",
      "kind": "video",
      "order": 0,
      "muted": false,
      "hidden": false,
      "clips": [
        {
          "id": "clip-uuid",
          "assetId": "asset-uuid",
          "timelineStartUs": 0,
          "sourceInUs": 1000000,
          "sourceOutUs": 6000000,
          "playbackRate": 1.0,
          "enabled": true
        }
      ]
    }
  ]
}
```

This is an illustrative initial schema, not a final public format.
Validate it when loading and saving.

### 6.3 Clip duration

For a normal-speed clip:

`source duration = sourceOutUs - sourceInUs`

`timeline duration = source duration / playbackRate`

`timeline end = timelineStartUs + timeline duration`

For MVP, keep playback rate fixed at `1.0` until speed-change behavior
is implemented and tested. Validate that source in/out points are within
the source media duration and that source out is greater than source in.

### 6.4 Timeline invariants

-   Every asset, track, and clip ID is unique within the project.
-   Every clip references an existing asset and track.
-   Source in/out points are valid.
-   Timeline start cannot be negative unless the product explicitly
    supports it.
-   Clip duration must be positive.
-   Project dimensions and frame rate must be valid.
-   A missing source file is a recoverable project warning, not a reason
    to discard the project.
-   Overlapping clips are permitted only according to track/compositing
    rules; the MVP must define deterministic behavior.
-   Mutations are validated before committing to project state.

------------------------------------------------------------------------

## 7. Rust Domain Types

The following is a starting point for `src-tauri/src/domain/project.rs`.
Adapt it as the schema evolves.

``` rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rational {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub audio_sample_rate: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub duration_us: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub asset_id: String,
    pub timeline_start_us: u64,
    pub source_in_us: u64,
    pub source_out_us: u64,
    pub playback_rate: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub order: u32,
    pub muted: bool,
    pub hidden: bool,
    pub clips: Vec<Clip>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub settings: ProjectSettings,
    pub assets: Vec<MediaAsset>,
    pub tracks: Vec<Track>,
}
```

For production, replace loosely typed `kind: String` values with enums,
validate all values, and use a typed ID wrapper or UUID type. Never
trust deserialized project data without validation.

------------------------------------------------------------------------

## 8. Media Import and Metadata

### 8.1 Import flow

1.  Open a native file picker.
2.  Receive selected file paths.
3.  Validate that each path exists and is a regular file.
4.  Run FFprobe with arguments passed separately---never concatenate
    paths into a shell command.
5.  Parse JSON metadata.
6.  Add a media asset to the project.
7.  Schedule thumbnail generation as a background job.
8.  Show a per-file status and recoverable errors.

### 8.2 FFprobe example

``` bash
ffprobe -v error \
  -show_format \
  -show_streams \
  -of json \
  "/path/to/input.mp4"
```

Do not assume all files contain a video stream, audio stream, or known
duration. Images may have no meaningful duration until placed on the
timeline. Variable-frame-rate media requires special care when mapping
timestamps to project frames.

### 8.3 Thumbnail generation

Use a low-resolution thumbnail for the media library and optionally a
thumbnail strip for timeline clips. Store generated assets in an
application cache, not beside original files by default. Cache keys
should include a stable source identity and relevant generation
parameters. Rebuild thumbnails when the source changes.

------------------------------------------------------------------------

## 9. Preview and Playback Strategy

Preview playback is a distinct engineering challenge; it should not be
treated as equivalent to final export.

### MVP approach

-   Start with still-frame preview at the playhead using FFmpeg frame
    extraction.
-   Add a playback component that can play a single source clip.
-   Implement timeline playback and synchronized multi-track compositing
    after the basic media workflow is stable.

### Important constraints

-   Frame extraction on every mouse-move event will be too expensive.
    Debounce scrubbing and cancel obsolete requests.
-   Use a preview cache for recently requested frames.
-   Never block the UI thread while decoding media.
-   Final export quality and real-time preview quality may differ.
-   A browser video element can be useful for basic single-clip preview,
    but multi-track compositing, codec support, and synchronization
    require deliberate design.
-   For advanced playback, evaluate a dedicated media playback library
    or a native decoding/rendering pipeline before committing to a large
    custom engine.

------------------------------------------------------------------------

## 10. Rendering and Export

### 10.1 Export pipeline

``` text
Project
  ↓
Validate project and locate assets
  ↓
Build a typed render plan
  ↓
Translate render plan into FFmpeg arguments/filtergraph
  ↓
Start background process
  ↓
Parse progress and stderr
  ↓
Publish progress events to UI
  ↓
Finalize output and report success/failure
```

### 10.2 Render-plan separation

Create a typed intermediate representation between the project model and
FFmpeg:

-   Input media list.
-   Trim/source timestamp mapping.
-   Track order and compositing rules.
-   Video output settings.
-   Audio mixing rules.
-   Filter operations.
-   Output container and codec settings.

This makes it easier to test render-plan generation without invoking
FFmpeg.

### 10.3 Simple single-clip export example

``` bash
ffmpeg -hide_banner -y \
  -ss 1.0 -i "/path/to/input.mp4" \
  -t 5.0 \
  -c:v libx264 -preset medium -crf 20 \
  -c:a aac -b:a 192k \
  -movflags +faststart \
  "/path/to/output.mp4"
```

This example trims one input; it is **not** the final multi-track
renderer. Seek placement can affect accuracy and performance. Production
rendering should build a tested filtergraph and handle timestamp
normalization, frame-rate conversion, scaling, audio gaps, overlays, and
encoding errors.

### 10.4 Process management requirements

-   Use Rust's process APIs with argument arrays.
-   Do not invoke a shell to interpolate untrusted paths.
-   Capture stderr and parse progress where possible.
-   Track each export as a job with an ID and state.
-   Support cancellation by terminating the child process and cleaning
    up temporary files.
-   Do not overwrite an existing output without explicit confirmation.
-   Export to a temporary output and rename after successful completion
    where practical.
-   Handle missing FFmpeg, unsupported codecs, disk-full errors, and
    permission errors.
-   Keep progress events bounded and avoid flooding the UI.

### 10.5 Export presets

Initial presets: - 720p, 30 FPS - 1080p, 30 FPS - 1080p, 60 FPS

Allow output frame rate and dimensions to be explicit. Do not promise
hardware acceleration until the installed FFmpeg build and GPU backend
are detected and tested.

------------------------------------------------------------------------

## 11. Tauri Commands and Events

Keep commands small and typed. Suggested initial commands:

  Command                     Purpose
  --------------------------- ------------------------------------------
  `create_project`            Create an empty project
  `open_project`              Load and validate a project file
  `save_project`              Persist current project state
  `import_media`              Probe and register selected media
  `get_media_metadata`        Return cached or fresh metadata
  `generate_thumbnail`        Request a thumbnail job
  `apply_edit`                Apply a validated timeline operation
  `undo_edit` / `redo_edit`   Reverse or reapply a command
  `start_export`              Validate project and start an export job
  `cancel_job`                Cancel a background job
  `get_job_status`            Query current job status

Suggested events: - `media-import-progress` - `thumbnail-ready` -
`project-dirty-changed` - `playback-frame-ready` - `export-progress` -
`export-completed` - `job-failed`

Event payloads should include stable IDs, such as `jobId`, `assetId`, or
`clipId`, to avoid confusing results from overlapping jobs.

Do not expose unrestricted filesystem access or arbitrary command
execution to the frontend. Configure Tauri capabilities narrowly and
validate all command arguments.

------------------------------------------------------------------------

## 12. UI Layout and Interaction Model

### Main editor layout

``` text
┌──────────────────────────────────────────────────────────────┐
│ Menu / Project Name / Undo / Redo / Export                  │
├──────────────┬────────────────────────────┬──────────────────┤
│ Media        │                            │ Properties       │
│ Library      │       Preview Monitor      │ Inspector        │
│              │                            │                  │
├──────────────┴────────────────────────────┴──────────────────┤
│ Timeline toolbar: zoom, snapping, track controls             │
├──────────────────────────────────────────────────────────────┤
│ Video track 1   [ Clip A ][ Clip B       ]                  │
│ Video track 2          [ Overlay / Text       ]             │
│ Audio track 1   [ Music -------------------- ]              │
│                         ▲ Playhead                           │
└──────────────────────────────────────────────────────────────┘
```

### Timeline interaction requirements

-   Drag media from the library to a track.
-   Select clips and show selection clearly.
-   Drag clips to move them.
-   Trim start/end with handles.
-   Scrub the playhead.
-   Zoom horizontally without changing clip timing.
-   Scroll vertically through tracks.
-   Provide snapping as a configurable behavior.
-   Keyboard shortcuts must not intercept typing in text inputs.
-   Offer accessible labels for controls and visible focus indicators.

Timeline rendering should be virtualized or otherwise optimized when
clip counts grow. Keep pixel-to-time conversion centralized and tested.

------------------------------------------------------------------------

## 13. Project Persistence and Recovery

-   Use a versioned JSON project file.
-   Save atomically: write a temporary file, flush as appropriate, then
    rename.
-   Keep a dirty/unsaved state.
-   Prompt before discarding unsaved changes.
-   Validate project data when opening it.
-   Provide migration functions when `schemaVersion` changes.
-   Consider periodic autosave and recovery snapshots after the basic
    save/load path works.
-   Store media paths in a documented way. Relative paths improve
    portability but need a clear base directory; absolute paths are
    simpler locally but less portable.
-   If media is missing, show a relink workflow rather than silently
    removing clips.

A later project-bundle format may package project metadata, proxy media,
thumbnails, and optional copied assets. Do not copy large source media
into bundles by default.

------------------------------------------------------------------------

## 14. Performance Strategy

1.  Generate thumbnails asynchronously.
2.  Cache metadata and preview frames.
3.  Use proxies for high-resolution media when necessary.
4.  Avoid transferring decoded frames over IPC.
5.  Batch timeline updates during drag interactions.
6.  Separate UI state from canonical project state.
7.  Limit concurrent FFmpeg jobs based on memory and CPU capacity.
8.  Measure before adding parallelism.
9.  Keep preview resolution lower than export resolution when
    appropriate.
10. Monitor temporary disk usage and clean up stale cache files safely.

Performance targets should be measured on documented test hardware. Do
not claim a fixed real-time playback rate until representative media,
codecs, and hardware have been benchmarked.

------------------------------------------------------------------------

## 15. Security and Reliability

-   Treat project files and media metadata as untrusted input.
-   Use argument arrays for FFmpeg/FFprobe; do not build shell command
    strings.
-   Restrict filesystem access through Tauri permissions/capabilities.
-   Validate paths, file types, project dimensions, frame rates, and
    timestamps.
-   Apply limits to metadata size, job concurrency, and generated cache
    data.
-   Avoid logging sensitive full paths unless useful and consented to.
-   Handle process crashes and cancellation without corrupting the
    project.
-   Test media files with missing streams, unusual timestamps, variable
    frame rates, and corrupted data.
-   Review FFmpeg redistribution and codec patent obligations for target
    markets.
-   Keep dependencies updated and review security advisories.

------------------------------------------------------------------------

## 16. Testing Strategy

### Rust tests

-   Clip duration calculations.
-   Timeline overlap and ordering rules.
-   Project validation.
-   Serialization and deserialization.
-   Schema migration.
-   Render-plan generation.
-   Path validation.
-   Job state transitions.

### Frontend tests

-   Timeline pixel-to-time calculations.
-   Clip selection and drag/trim interactions.
-   Undo/redo controls.
-   Unsaved-change prompts.
-   Export progress and failure states.
-   Keyboard shortcuts and accessibility behavior.

### Integration tests

-   Probe a known test video.
-   Generate a thumbnail.
-   Create a project, add a clip, save, reopen, and verify the data.
-   Export a short sample and verify that the output exists and is
    readable.
-   Cancel a long export and verify that no corrupt final file is
    reported as complete.

Use small, redistributable fixtures with clear licensing. Include tests
for missing files and unavailable FFmpeg.

------------------------------------------------------------------------

## 17. Development Milestones

### Milestone 0 --- Foundation

-   Install Rust, Node.js, pnpm, Tauri prerequisites, and FFmpeg.
-   Scaffold the Tauri + React + TypeScript app.
-   Add logging, error handling, and a basic CI workflow.
-   Verify the app launches locally.

**Exit criteria:** A reproducible local development setup and successful
desktop launch.

### Milestone 1 --- Project and media library

-   Define project schema and validation.
-   Create/open/save projects.
-   Import files and probe metadata.
-   Generate and display thumbnails.

**Exit criteria:** A project can be saved and reopened with imported
assets intact.

### Milestone 2 --- Timeline editing

-   Implement tracks and clips.
-   Add placement, move, trim, selection, snapping, and zoom.
-   Add undo/redo using command-based edits.

**Exit criteria:** A user can assemble and save a basic sequence.

### Milestone 3 --- Preview

-   Add playhead and seeking.
-   Implement still-frame preview and caching.
-   Add single-clip playback, then timeline preview.

**Exit criteria:** The preview behavior is predictable for supported
test media.

### Milestone 4 --- Export

-   Implement render-plan generation.
-   Build and test FFmpeg arguments/filtergraphs.
-   Add progress, cancellation, and output validation.

**Exit criteria:** A basic timeline exports to a playable MP4.

### Milestone 5 --- Editing features

-   Audio controls, text overlays, transitions, and image overlays.
-   Better preview performance and proxy media.
-   More export presets.

**Exit criteria:** The editor supports a practical short-form editing
workflow.

### Milestone 6 --- Advanced features

-   Keyframes, captions, effects, templates, and AI-assisted workflows.
-   Crash recovery, packaging, signing, and release automation.

**Exit criteria:** Feature-specific tests, packaging, and documentation
are complete.

------------------------------------------------------------------------

## 18. Developer Setup

### Prerequisites

Install the current stable Rust toolchain, a supported Node.js release,
pnpm, Git, FFmpeg, and the native dependencies required by Tauri for the
target operating system. Follow the current Tauri prerequisites guide
for the exact packages on your distribution.

On Fedora, package names can vary by release and enabled repositories.
Confirm package availability before assuming a command works:

``` bash
sudo dnf install gcc gcc-c++ make pkg-config openssl-devel
```

Install Rust if it is not already available:

``` bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustc --version
cargo --version
```

Check media tools:

``` bash
ffmpeg -version
ffprobe -version
```

Scaffold the application:

``` bash
pnpm create tauri-app
```

Choose the React + TypeScript template and pnpm when prompted. Then use
the generated project's actual package scripts:

``` bash
cd entrocut
pnpm install
pnpm tauri dev
```

The exact generated directory and script names may differ based on the
scaffolding choices. Verify the generated `package.json` before adding
custom scripts.

### Suggested scripts

Use the scripts created by the scaffold as the source of truth. A
typical setup may include: - `dev`: start Vite - `build`: type-check and
build frontend - `tauri`: run Tauri CLI - `test`: run frontend tests -
`test:rust`: run `cargo test` in `src-tauri`

Do not copy scripts blindly if the generated template uses different
names.

------------------------------------------------------------------------

## 19. Definition of Done for MVP

The MVP is complete when all of the following are true:

-   [ ] Application launches on the primary target Linux distribution.
-   [ ] User can create, save, close, and reopen a project.
-   [ ] User can import at least video, audio, and image assets where
    supported.
-   [ ] Metadata and thumbnails are generated without freezing the UI.
-   [ ] User can add clips to tracks, move them, and trim their source
    range.
-   [ ] Project state survives restart.
-   [ ] A basic timeline can be previewed with documented limitations.
-   [ ] Export creates a playable MP4 with selected output settings.
-   [ ] Export progress, cancellation, and failure are surfaced
    accurately.
-   [ ] Missing media is reported and can be relinked or resolved.
-   [ ] Core timeline calculations and project validation have automated
    tests.
-   [ ] FFmpeg distribution/licensing and third-party notices have been
    reviewed before release.

------------------------------------------------------------------------

## 20. Risks and Open Decisions

  -----------------------------------------------------------------------
  Risk / decision                     Mitigation
  ----------------------------------- -----------------------------------
  Real-time multi-track playback is   Ship still-frame and single-clip
  complex                             preview first; evaluate a dedicated
                                      playback engine

  FFmpeg build differences            Detect capabilities and report
                                      unsupported codecs clearly

  Variable-frame-rate media           Use timestamps carefully and test
                                      representative files

  Large 4K/8K assets                  Add proxies and bounded concurrency

  Project portability                 Define relative/absolute path
                                      policy and media relinking

  Export performance                  Benchmark before adding hardware
                                      acceleration

  FFmpeg licensing                    Review exact build configuration
                                      and distribution obligations

  Project schema evolution            Version the format and test
                                      migrations

  Scope growth                        Require each milestone to meet exit
                                      criteria before adding advanced
                                      features
  -----------------------------------------------------------------------

------------------------------------------------------------------------

## 21. Official References

Check the latest official documentation because APIs, package
requirements, and release behavior can change.

-   Tauri documentation: https://tauri.app/
-   Tauri v2 guides: https://v2.tauri.app/
-   Rust documentation: https://doc.rust-lang.org/
-   React documentation: https://react.dev/
-   TypeScript documentation: https://www.typescriptlang.org/docs/
-   Vite documentation: https://vite.dev/guide/
-   FFmpeg documentation: https://ffmpeg.org/documentation.html
-   FFprobe documentation: https://ffmpeg.org/ffprobe.html
-   pnpm documentation: https://pnpm.io/

------------------------------------------------------------------------

## 22. Recommended Next Step

Start with **Milestone 0 and Milestone 1 only**. Scaffold the app,
launch it, define the versioned project schema, then implement project
save/load and media probing. Avoid implementing a complex real-time
playback engine before the project model and media pipeline are
testable.

The architectural principle is simple: **React presents the editing
experience, Rust owns project correctness and background jobs, and
FFmpeg handles media processing.** This separation keeps EntroCut
extensible without requiring a custom codec and rendering engine on day
one.
