# Ghidra scripts for the openwa-re pipeline

Drop these into your Ghidra `GhidraScripts` search path (default
`~/ghidra_scripts/`) or symlink the directory there. After Ghidra picks
them up, tick "In Tool" in Script Manager and they appear under
`Tools → OpenWA → Export catalog` / `Import catalog`. Also runnable
headless via `analyzeHeadless` and over the MCP bridge.

Both scripts pair with `cargo run -p openwa-re-data --bin openwa-re`.
The verbs are named from each side's local perspective, so they mirror:

| Direction | Ghidra-side script | Rust-side command |
| --- | --- | --- |
| Ghidra DB → committed `re/` | `OpenWAExport.java` | `openwa-re import` |
| committed `re/` → Ghidra DB | `OpenWAImport.java` | `openwa-re export` |

## `OpenWAExport.java` — Ghidra → disk

Writes `<prefix>.xml` (Ghidra's native overlay format) plus
`<prefix>_extras.json` (calling convention / no-return / custom-storage
flags per function; Ghidra's XML DTD cannot carry these). Default
prefix is `C:/tmp/wa_export`; pass a different prefix as the only arg.

Feed the output into the Rust tool to refresh / bootstrap `re/`:

```text
cargo run -p openwa-re-data --bin openwa-re -- \
    import --bootstrap <prefix>.xml
```

## `OpenWAImport.java` — disk → Ghidra

Reads a single JSON manifest and applies every entry to the
currently-open program via Ghidra's Java API: DTM, symbols, function
prototypes (incl. custom storage), comments, typed globals, plus the
calling-convention / no-return / custom-storage flags. Default
manifest path is `C:/tmp/wa_import.json`.

The Rust tool generates the manifest from `re/`:

```text
cargo run -p openwa-re-data --bin openwa-re -- \
    export --out C:/tmp/wa_import.json \
    --extras C:/tmp/wa_export_extras.json
```

`--extras` is optional but required to round-trip calling
conventions and custom-storage flags through `re/` (which doesn't
yet store them in TOML directly).

## Onboarding (target)

1. Install Ghidra 11+ and import WA.exe; run auto-analysis.
2. `cargo run -p openwa-re-data --bin openwa-re -- export --out C:/tmp/wa_import.json --extras <sidecar>` — produces the manifest.
3. In Ghidra: `Tools → OpenWA → Import catalog` (or `OpenWAImport.java C:/tmp/wa_import.json` headless).
4. Open any function — names, types, custom storage, comments are populated.
