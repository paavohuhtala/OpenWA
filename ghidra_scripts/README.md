# Ghidra scripts for the openwa-re pipeline

Drop these into your Ghidra `GhidraScripts` search path (default
`~/ghidra_scripts/`) or symlink the directory there. Run via Ghidra's
Script Manager (or the `analyzeHeadless` CLI) against your WA.exe project.

Both scripts pair with `cargo run -p openwa-re-data --bin openwa-re`.

## Export — Ghidra → `re/`

`ReExport.java` writes `<prefix>.xml` (Ghidra's native overlay format)
plus `<prefix>_extras.json` (calling convention + no-return per function;
Ghidra's XML DTD cannot carry these). Default prefix is
`C:/tmp/wa_export`; pass a different prefix as the only arg.

Feed the output into the Rust tool to produce a fresh `re/` tree:

```text
cargo run -p openwa-re-data --bin openwa-re -- export --bootstrap <prefix>.xml
```

## Import — `re/` → Ghidra

`ReImport.java` reads `<prefix>.xml` + `<prefix>_extras.json` and applies
them to the **currently-open** Ghidra program: DTM, symbols, function
prototypes (incl. custom storage), comments, typed globals, plus the
calling-convention / no-return fields from the sidecar.

The Rust tool generates both files from `re/`:

```text
cargo run -p openwa-re-data --bin openwa-re -- import --out <prefix>
```

Then run `ReImport.java <prefix>` inside Ghidra.

## Onboarding (target)

1. Install Ghidra 11+ and import WA.exe; run auto-analysis.
2. `cargo run -p openwa-re-data --bin openwa-re -- import --out C:/tmp/wa_import`
3. In Ghidra, run `ReImport.java C:/tmp/wa_import`.
4. Open any function — names, types, custom storage, comments are populated.
