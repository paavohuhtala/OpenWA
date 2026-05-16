// openwa-re import-side bridge.
//
// Applies the output of `openwa-re import --out <prefix>` to the current
// Ghidra program:
//   1) ProgramXmlMgr against `<prefix>.xml` — DTM, symbols (function +
//      global names), comments, typed globals.
//   2) `<prefix>_extras.json` — every per-function override: plate
//      comment, calling convention, no-return, return type, parameters
//      (including custom storage strings like "ECX" / "EDX:EAX" /
//      "stack:0x4"). Applied via `Function.updateFunction(...)` rather
//      than the XML `<FUNCTION>` element, because Ghidra's
//      `FunctionsXmlMgr.read` NPEs on every entry whose address already
//      holds a function with bodySize > 1.
//
// Usage: pass the prefix path (no extension) as an arg:
//   ReImport.java C:/tmp/wa_import
// Defaults to `C:/tmp/wa_import` if no arg is given.
//
// @category OpenWA

import com.google.gson.Gson;
import com.google.gson.annotations.SerializedName;
import ghidra.app.script.GhidraScript;
import ghidra.app.util.importer.MessageLog;
import ghidra.app.util.xml.ProgramXmlMgr;
import ghidra.app.util.xml.XmlProgramOptions;
import ghidra.program.model.address.Address;
import ghidra.program.model.data.DataType;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Function.FunctionUpdateType;
import ghidra.program.model.listing.Parameter;
import ghidra.program.model.listing.ParameterImpl;
import ghidra.program.model.listing.Program;
import ghidra.program.model.listing.ReturnParameterImpl;
import ghidra.program.model.listing.Variable;
import ghidra.program.model.listing.VariableStorage;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolTable;
import ghidra.util.data.DataTypeParser;
import ghidra.util.data.DataTypeParser.AllowedDataTypes;
import java.io.File;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;

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
        opts.setAddToProgram(true);
        // Symbols + functions are applied via the extras sidecar (see
        // applyExtras / applySymbols). Ghidra's XmlMgr paths NPE on edge
        // cases without surfacing the failing entry; applying via the
        // API gives us per-entry error reporting.
        opts.setSymbols(false);
        opts.setFunctions(false);
        opts.setData(true);
        opts.setComments(true);
        opts.setReferences(true);
        opts.setEquates(true);
        opts.setExternalLibraries(false);
        opts.setMemoryBlocks(false);
        opts.setMemoryContents(false);
        opts.setInstructions(false);
        opts.setRelocationTable(false);
        opts.setTrees(false);
        opts.setEntryPoints(false);
        opts.setRegisters(false);
        opts.setBookmarks(false);
        opts.setProperties(false);
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
        String json = Files.readString(extras.toPath());
        Gson gson = new Gson();
        Extras ex = gson.fromJson(json, Extras.class);
        if (ex == null) {
            println("Extras sidecar empty.");
            return;
        }
        applySymbols(ex);
        applyExtras(ex);
    }

    private static class Extras {
        List<SymbolExtras> symbols;
        List<FunctionExtras> functions;
    }

    private static class SymbolExtras {
        String va;
        String name;
        String kind; // function | data | label
    }

    private static class FunctionExtras {
        String va;
        @SerializedName("calling_convention") String callingConvention;
        @SerializedName("no_return") boolean noReturn;
        @SerializedName("plate_comment") String plateComment;
        @SerializedName("return_type") String returnType;
        List<ParamExtras> params;
    }

    private static class ParamExtras {
        String name;
        String type;
        String storage;
    }

    private void applySymbols(Extras ex) throws Exception {
        if (ex.symbols == null || ex.symbols.isEmpty()) {
            return;
        }
        SymbolTable symbolTable = currentProgram.getSymbolTable();
        int applied = 0, skipped = 0, errors = 0;
        long t0 = System.currentTimeMillis();
        for (SymbolExtras se : ex.symbols) {
            if (monitor.isCancelled()) break;
            long va = Long.parseLong(se.va.substring(2), 16);
            Address addr = currentProgram.getAddressFactory()
                    .getDefaultAddressSpace().getAddress(va);
            try {
                Symbol primary = symbolTable.getPrimarySymbol(addr);
                if (primary != null && se.name.equals(primary.getName())
                        && primary.getParentNamespace().isGlobal()) {
                    // Already correct — no work.
                    skipped++;
                    continue;
                }
                // Function path: rename in place if a function exists here.
                Function func = currentProgram.getFunctionManager().getFunctionAt(addr);
                if (func != null) {
                    func.setName(se.name, SourceType.USER_DEFINED);
                    applied++;
                    continue;
                }
                // Non-function: ensure a primary label with the desired name.
                if (primary != null) {
                    primary.setName(se.name, SourceType.USER_DEFINED);
                } else {
                    symbolTable.createLabel(addr, se.name, SourceType.USER_DEFINED);
                }
                applied++;
            } catch (Exception e) {
                errors++;
                printerr("Symbol " + addr + " <- \"" + se.name + "\" ("
                        + se.kind + "): " + e.getClass().getSimpleName()
                        + ": " + e.getMessage());
            }
        }
        long dt = System.currentTimeMillis() - t0;
        println("Symbols: applied " + applied + " (skipped " + skipped
                + " already-correct, " + errors + " error(s)) in " + dt + " ms.");
    }

    private void applyExtras(Extras ex) throws Exception {
        if (ex.functions == null || ex.functions.isEmpty()) {
            return;
        }
        DataTypeParser dtParser = new DataTypeParser(
            currentProgram.getDataTypeManager(),
            currentProgram.getDataTypeManager(),
            null,
            AllowedDataTypes.DYNAMIC);

        int applied = 0, notFound = 0, errors = 0;
        long t0 = System.currentTimeMillis();
        for (FunctionExtras fe : ex.functions) {
            if (monitor.isCancelled()) break;
            long va = Long.parseLong(fe.va.substring(2), 16);
            Address addr = currentProgram.getAddressFactory()
                    .getDefaultAddressSpace().getAddress(va);
            Function func = currentProgram.getFunctionManager().getFunctionAt(addr);
            if (func == null) {
                notFound++;
                continue;
            }
            try {
                applyOne(fe, func, dtParser);
                applied++;
            } catch (Exception e) {
                errors++;
                printerr("Extras at " + addr + ": " + e.getClass().getSimpleName()
                        + ": " + e.getMessage());
            }
        }
        long dt = System.currentTimeMillis() - t0;
        println("Extras: applied " + applied + " / " + ex.functions.size()
                + " function(s) in " + dt + " ms; "
                + notFound + " VA(s) had no function; "
                + errors + " error(s).");
    }

    private void applyOne(FunctionExtras fe, Function func, DataTypeParser dtParser)
            throws Exception {
        // Plate / regular comment — Function.setComment sets the slot
        // FunctionsXmlMgr maps to <REGULAR_CMT>; shown in the listing header.
        if (fe.plateComment != null) {
            func.setComment(fe.plateComment);
        }

        boolean hasParams = fe.params != null && !fe.params.isEmpty();
        boolean hasReturn = fe.returnType != null;
        boolean anyCustomStorage = false;
        if (hasParams) {
            for (ParamExtras p : fe.params) {
                if (p.storage != null) {
                    anyCustomStorage = true;
                    break;
                }
            }
        }

        if (hasParams || hasReturn) {
            Variable returnVar = null;
            if (hasReturn) {
                DataType rt = dtParser.parse(fe.returnType);
                returnVar = new ReturnParameterImpl(rt, currentProgram);
            }

            List<Variable> newParams = new ArrayList<>();
            if (hasParams) {
                for (ParamExtras p : fe.params) {
                    DataType dt = dtParser.parse(p.type);
                    ParameterImpl param;
                    if (anyCustomStorage) {
                        VariableStorage vs = (p.storage != null)
                                ? parseStorage(p.storage, dt.getLength(), currentProgram)
                                : VariableStorage.UNASSIGNED_STORAGE;
                        param = new ParameterImpl(p.name, dt, vs, currentProgram);
                    } else {
                        param = new ParameterImpl(p.name, dt, currentProgram);
                    }
                    newParams.add(param);
                }
            }

            FunctionUpdateType updateType = anyCustomStorage
                    ? FunctionUpdateType.CUSTOM_STORAGE
                    : FunctionUpdateType.DYNAMIC_STORAGE_FORMAL_PARAMS;
            func.updateFunction(fe.callingConvention, returnVar, newParams,
                    updateType, true, SourceType.USER_DEFINED);
        } else if (fe.callingConvention != null) {
            func.setCallingConvention(fe.callingConvention);
        }

        if (fe.noReturn) {
            func.setNoReturn(true);
        }
    }

    /**
     * Parse a storage spec from the TOML side. Mirrors the on-disk grammar:
     *   "ECX"          → single register
     *   "EDX:EAX"      → multi-register pair (high:low or whatever Ghidra's
     *                    register order is for the architecture)
     *   "stack:0x4"    → stack slot at offset 4, size from the datatype
     *   "stack:0x10:4" → stack slot at offset 16, explicit byte size
     */
    private VariableStorage parseStorage(String spec, int dtSize, Program program)
            throws Exception {
        if (spec.startsWith("stack:")) {
            String rest = spec.substring("stack:".length());
            int colon = rest.indexOf(':');
            int offset;
            int size;
            if (colon >= 0) {
                offset = parseSignedHexOrDec(rest.substring(0, colon));
                size = Integer.parseInt(rest.substring(colon + 1));
            } else {
                offset = parseSignedHexOrDec(rest);
                size = dtSize;
            }
            if (size <= 0) {
                throw new IllegalArgumentException(
                    "stack storage " + spec + ": datatype has no size; "
                    + "specify size explicitly as stack:" + rest + ":<size>");
            }
            return new VariableStorage(program, offset, size);
        }
        if (spec.indexOf(':') >= 0) {
            String[] parts = spec.split(":");
            Register[] regs = new Register[parts.length];
            for (int i = 0; i < parts.length; i++) {
                regs[i] = program.getRegister(parts[i]);
                if (regs[i] == null) {
                    throw new IllegalArgumentException(
                        "unknown register `" + parts[i] + "` in storage " + spec);
                }
            }
            return new VariableStorage(program, regs);
        }
        Register reg = program.getRegister(spec);
        if (reg == null) {
            throw new IllegalArgumentException("unknown register `" + spec + "`");
        }
        return new VariableStorage(program, reg);
    }

    private static int parseSignedHexOrDec(String s) {
        boolean neg = s.startsWith("-");
        String body = neg ? s.substring(1) : s;
        int v = body.startsWith("0x") || body.startsWith("0X")
                ? Integer.parseInt(body.substring(2), 16)
                : Integer.parseInt(body);
        return neg ? -v : v;
    }
}
