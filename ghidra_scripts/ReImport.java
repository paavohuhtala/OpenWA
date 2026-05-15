// openwa-re import-side bridge.
//
// Applies the output of `openwa-re import --out <prefix>` to the current
// Ghidra program:
//   1) Runs ProgramXmlMgr against `<prefix>.xml` — DTM, symbols, function
//      prototypes (including custom storage via REGISTER_VAR), comments,
//      typed globals.
//   2) Reads `<prefix>_extras.json` and applies calling_convention /
//      no_return per function (XML DTD can't carry these).
//
// Usage: pass the prefix path (no extension) as an arg:
//   ReImport.java C:/tmp/wa_import
// Defaults to `C:/tmp/wa_import` if no arg is given.
//
// @category OpenWA

import ghidra.app.script.GhidraScript;
import ghidra.app.util.importer.MessageLog;
import ghidra.app.util.xml.ProgramXmlMgr;
import ghidra.app.util.xml.XmlProgramOptions;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import java.io.File;
import java.nio.file.Files;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

public class ReImport extends GhidraScript {
    @Override
    public void run() throws Exception {
        String prefix;
        String[] args = getScriptArgs();
        if (args != null && args.length > 0) {
            prefix = args[0];
        } else {
            prefix = "C:/tmp/wa_import";
        }

        // ─── Step 1: XML overlay via ProgramXmlMgr ───────────────────────────
        File xml = new File(prefix + ".xml");
        if (!xml.isFile()) {
            printerr("Missing " + xml.getAbsolutePath() + " — run `openwa-re import` first.");
            return;
        }
        long t0 = System.currentTimeMillis();
        ProgramXmlMgr mgr = new ProgramXmlMgr(xml);
        XmlProgramOptions opts = new XmlProgramOptions();
        // Apply over an existing program (don't try to create memory).
        opts.setAddToProgram(true);
        // Apply everything our XML carries.
        opts.setSymbols(true);
        opts.setFunctions(true);
        opts.setData(true);
        opts.setComments(true);
        opts.setReferences(true);
        opts.setEquates(true);
        opts.setExternalLibraries(false);
        // Things derived from the binary — leave off, we don't write them.
        opts.setMemoryBlocks(false);
        opts.setMemoryContents(false);
        opts.setInstructions(false);
        opts.setRelocationTable(false);
        opts.setTrees(false);
        opts.setEntryPoints(false);
        opts.setRegisters(false);
        opts.setBookmarks(false);
        opts.setProperties(false);
        // Overwrite on conflict — the TOML is the source of truth.
        opts.setOverwriteSymbolConflicts(true);
        opts.setOverwriteDataConflicts(true);
        opts.setOverwriteReferenceConflicts(true);
        opts.setOverwritePropertyConflicts(true);

        MessageLog log = mgr.read(currentProgram, monitor, opts);
        long dt = System.currentTimeMillis() - t0;
        println("XML applied in " + dt + " ms");
        String logText = log == null ? "" : log.toString();
        if (!logText.isEmpty()) {
            println("XML import log:\n" + logText);
        }

        // ─── Step 2: extras sidecar ──────────────────────────────────────────
        File extras = new File(prefix + "_extras.json");
        if (!extras.isFile()) {
            println("No extras sidecar at " + extras.getAbsolutePath() + " — skipping.");
            return;
        }
        applyExtras(Files.readString(extras.toPath()));
    }

    private void applyExtras(String json) throws Exception {
        // The sidecar is intentionally simple — one object per function,
        // `va`, `calling_convention?`, `no_return?`. Scan with a flat regex
        // rather than pulling in a JSON dependency.
        Pattern p = Pattern.compile(
            "\\{\\s*\"va\"\\s*:\\s*\"(0x[0-9A-Fa-f]+)\""
            + "(?:\\s*,\\s*\"calling_convention\"\\s*:\\s*\"([^\"]+)\")?"
            + "(?:\\s*,\\s*\"no_return\"\\s*:\\s*(true|false))?"
            + "\\s*\\}"
        );
        Matcher m = p.matcher(json);
        int applied = 0;
        int unknown = 0;
        int ccFailures = 0;
        while (m.find()) {
            if (monitor.isCancelled()) break;
            long va = Long.parseLong(m.group(1).substring(2), 16);
            Address addr = currentProgram.getAddressFactory()
                    .getDefaultAddressSpace().getAddress(va);
            Function f = currentProgram.getFunctionManager().getFunctionAt(addr);
            if (f == null) {
                unknown++;
                continue;
            }
            String cc = m.group(2);
            String nr = m.group(3);
            if (cc != null) {
                try {
                    f.setCallingConvention(cc);
                } catch (Exception e) {
                    ccFailures++;
                    printerr("setCallingConvention(" + cc + ") at "
                            + addr + " failed: " + e.getMessage());
                }
            }
            if ("true".equals(nr)) {
                f.setNoReturn(true);
            }
            applied++;
        }
        println("Extras: applied " + applied + " function(s); "
                + unknown + " VA(s) had no function; "
                + ccFailures + " convention failure(s).");
    }
}
