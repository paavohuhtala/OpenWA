# openwa-asset-viewer

Standalone egui application for browsing WA's on-disk asset formats. Builds as the `openwa-asset-viewer` binary; lives outside the DLL/launcher stack and never touches WA.exe.

## Scope

- `.img` images — rendered via `openwa_core::img::img_decode` (see `image_viewer.rs`).
- `.pal` palettes — RIFF PAL, decoded via `openwa_core::pal::pal_decode` (see `palette_viewer.rs`). Sparse palettes (most entries zero) are normal; each file populates one sub-range of the runtime palette.
- `.dir` archives — listed via `openwa_core::dir::dir_decode` (see `archive_viewer.rs`). Per-row Open buttons extract `.img` / `.pal` entries and spawn nested viewer windows through a `PendingOpen` queue drained each frame by `AssetViewer::ui` in `main.rs`.

## Design notes

- All parsing lives in `openwa-core`. This crate does **not** depend on `openwa-game` and must not — it's the portable consumer of those parsers and the demo surface for new formats as they're added.
- `ImageViewer` / `PaletteViewer` have both `open(path)` and `open_bytes(...)` constructors so the same decoding path serves disk-loaded files and archive-extracted blobs.
- `recent.rs` stores the last-10 disk-loaded paths at `<dirs::data_dir>/OpenWA/recent_files.txt` (UTF-8, one path per line). Only actual file-system opens are tracked — archive-extracted blobs aren't.
- Large directory listings (`Gfx*.dir` ≈ 1000 entries) use `egui_extras::TableBuilder` with virtualized rows; `egui::Grid` isn't virtualized and stutters at that scale.

## Adding a new format

1. Add the decoder to `openwa-core` (pure `&[u8] -> DecodedThing`, no I/O).
2. Add a viewer module here with `open(path)` + `open_bytes(...)`.
3. Register the extension in `AssetViewer::open_path` in `main.rs`.
4. If it should be openable from inside a `.dir` listing, extend `archive_viewer::known_kind` and `PendingOpenKind`.
