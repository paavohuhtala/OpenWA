// Export the current program's RE catalog to Ghidra XML + sidecar.
//
// Runs Ghidra's native XmlExporter to produce a `desired.xml` snapshot, then
// walks every USER_DEFINED function and emits a sidecar JSON listing each
// function's calling_convention / no_return / custom_storage — fields
// Ghidra's XML DTD cannot carry. The Rust `openwa-re import` command
// consumes both files.
//
// Output paths default to C:/tmp/wa_export.xml + C:/tmp/wa_export_extras.json.
// Pass an alternative prefix via args (no extension): `wa_export` becomes
// `<prefix>.xml` + `<prefix>_extras.json`.
//
// @category OpenWA
// @menupath Tools.OpenWA.Export catalog
// @toolbar

import ghidra.app.script.GhidraScript;
import ghidra.app.util.exporter.XmlExporter;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;
import ghidra.program.model.symbol.SourceType;
import ghidra.util.task.TaskMonitor;
import java.io.File;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public class OpenWAExport extends GhidraScript {
    @Override
    public void run() throws Exception {
        String prefix;
        String[] args = getScriptArgs();
        if (args != null && args.length > 0) {
            prefix = args[0];
        } else {
            prefix = "C:/tmp/wa_export";
        }

        // ─── XML overlay ─────────────────────────────────────────────────────
        File xmlOut = new File(prefix + ".xml");
        XmlExporter exporter = new XmlExporter();
        long t0 = System.currentTimeMillis();
        boolean ok = exporter.export(xmlOut, currentProgram, null, monitor);
        long dt = System.currentTimeMillis() - t0;
        if (!ok) {
            printerr("XmlExporter.export returned false; aborting");
            return;
        }
        println("Wrote " + xmlOut.getAbsolutePath()
                + " (" + xmlOut.length() + " bytes) in " + dt + " ms");

        // ─── Sidecar JSON: per-function metadata XML can't carry ─────────────
        // We emit `calling_convention` for every user-defined function whose
        // convention is concretely set (not `unknown`), even when it matches
        // the program default. The Rust side's validator requires every
        // function with params to declare a convention explicitly — a
        // round-trip that strips it would re-introduce the bug the validator
        // is there to catch.
        List<String> entries = new ArrayList<>();
        FunctionIterator fns = currentProgram.getFunctionManager().getFunctions(true);
        while (fns.hasNext()) {
            if (monitor.isCancelled()) break;
            Function f = fns.next();
            if (f.getSymbol() == null) continue;
            SourceType src = f.getSymbol().getSource();
            // Thunks (CRT/MFC import wrappers like `WA_Free`, `_atol`,
            // `__fputchar`, `AfxInternalProcessWndProcException`) carry
            // their names from PE-loader auto-analysis, so Ghidra reports
            // their symbol source as DEFAULT — they would otherwise be
            // filtered out here and lose their calling convention on
            // round-trip. Let thunks through unconditionally; the
            // attribute filter below still skips them if there's nothing
            // to emit.
            if (src != SourceType.USER_DEFINED
                    && src != SourceType.IMPORTED
                    && !f.isThunk()) {
                continue;
            }

            String cc = f.getCallingConventionName();
            // For thunks where the thunk itself has no cc, fall back to
            // the thunk target's cc.
            if ((cc == null || cc.equals("unknown")) && f.isThunk()) {
                Function tgt = f.getThunkedFunction(true);
                if (tgt != null) {
                    String tcc = tgt.getCallingConventionName();
                    if (tcc != null && !tcc.equals("unknown")) {
                        cc = tcc;
                    }
                }
            }
            boolean noReturn = f.hasNoReturn();
            boolean customStorage = f.hasCustomVariableStorage();
            boolean ccKnown = cc != null && !cc.equals("unknown");
            if (!ccKnown && !noReturn && !customStorage) continue;

            StringBuilder e = new StringBuilder();
            e.append("    {");
            e.append("\"va\": \"0x").append(String.format("%08X", f.getEntryPoint().getOffset())).append("\"");
            if (ccKnown) {
                e.append(", \"calling_convention\": ").append(jsonString(normaliseCc(cc)));
            }
            if (noReturn) {
                e.append(", \"no_return\": true");
            }
            if (customStorage) {
                e.append(", \"custom_storage\": true");
            }
            e.append("}");
            entries.add(e.toString());
        }
        Collections.sort(entries);

        File jsonOut = new File(prefix + "_extras.json");
        StringWriter sw = new StringWriter();
        PrintWriter pw = new PrintWriter(sw);
        pw.println("{");
        pw.println("  \"functions\": [");
        for (int i = 0; i < entries.size(); i++) {
            pw.print(entries.get(i));
            pw.println(i == entries.size() - 1 ? "" : ",");
        }
        pw.println("  ]");
        pw.println("}");
        pw.flush();
        java.nio.file.Files.writeString(jsonOut.toPath(), sw.toString());
        println("Wrote " + jsonOut.getAbsolutePath()
                + " — " + entries.size() + " function(s) with non-default attrs");
    }

    /// Ghidra exposes conventions as raw names (`__stdcall`, `__thiscall`,
    /// `__cdecl`, `__fastcall`); some custom ones come through as plain
    /// `unknown`. Pass them through verbatim, but normalise any
    /// Ghidra-internal name to the `__xxx` flavour our TOML schema uses.
    private String normaliseCc(String cc) {
        if (cc == null) return "__stdcall";
        if (cc.startsWith("__")) return cc;
        return "__" + cc;
    }

    private static String jsonString(String s) {
        StringBuilder out = new StringBuilder();
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"':  out.append("\\\""); break;
                case '\\': out.append("\\\\"); break;
                case '\n': out.append("\\n"); break;
                case '\r': out.append("\\r"); break;
                case '\t': out.append("\\t"); break;
                default:
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    } else {
                        out.append(c);
                    }
            }
        }
        out.append('"');
        return out.toString();
    }
}
