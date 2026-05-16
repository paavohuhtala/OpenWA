// openwa-re import-side bridge.
//
// Consumes a single JSON manifest produced by `openwa-re import --out <path>`
// and applies every entry via Ghidra's Java API. We deliberately avoid
// Ghidra's XML import managers: each of FunctionsXmlMgr, SymbolTableXmlMgr,
// and DataTypesXmlMgr NPEs / IAEs / `.conflict`-spams on common edge
// cases when overlaying onto an existing DB, and silently discards the
// failing element. Going through the API directly gives us per-entry
// error reporting and full control over conflict handling.
//
// Order of operations:
//   1. Types — typedefs / enums / unions / structs / function-defs
//      (two-pass: shells first, members second, so cycles + forward
//      references resolve cleanly).
//   2. Typed globals — applies a DataType at a VA.
//   3. Comments — Listing.setComment per (address, kind).
//   4. Create missing functions — for any function-kind symbol at a VA
//      where Ghidra hasn't auto-identified a Function, run
//      CreateFunctionCmd so the subsequent symbol-rename and metadata
//      passes have a target.
//   5. Symbols — (VA, name) renames for functions, data, labels.
//   6. Function metadata — plate comment, signature, params, custom
//      storage, calling convention, no-return.
//
// Usage:
//   ReImport.java C:/tmp/wa_import.json
// (defaults to C:/tmp/wa_import.json if no arg)
//
// @category OpenWA

import com.google.gson.Gson;
import com.google.gson.annotations.SerializedName;
import ghidra.app.cmd.disassemble.DisassembleCommand;
import ghidra.app.cmd.function.CreateFunctionCmd;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.address.AddressSet;
import ghidra.program.model.data.Category;
import ghidra.program.model.data.CategoryPath;
import ghidra.program.model.data.DataType;
import ghidra.program.model.data.DataTypeComponent;
import ghidra.program.model.data.DataTypeConflictHandler;
import ghidra.program.model.data.DataTypeManager;
import ghidra.program.model.data.EnumDataType;
import ghidra.program.model.data.FunctionDefinitionDataType;
import ghidra.program.model.data.ParameterDefinition;
import ghidra.program.model.data.ParameterDefinitionImpl;
import ghidra.program.model.data.Structure;
import ghidra.program.model.data.StructureDataType;
import ghidra.program.model.data.TypedefDataType;
import ghidra.program.model.data.Union;
import ghidra.program.model.data.UnionDataType;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.CodeUnit;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.Function.FunctionUpdateType;
import ghidra.program.model.listing.Listing;
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
    private DataTypeManager dtm;
    private DataTypeParser dtParser;

    @Override
    public void run() throws Exception {
        String path;
        String[] args = getScriptArgs();
        if (args != null && args.length > 0) {
            path = args[0];
        } else {
            path = "C:/tmp/wa_import.json";
        }
        File file = new File(path);
        if (!file.isFile()) {
            printerr("Missing manifest at " + file.getAbsolutePath()
                    + " — run `openwa-re import --out " + path + "` first.");
            return;
        }
        String json = Files.readString(file.toPath());
        Manifest m = new Gson().fromJson(json, Manifest.class);
        if (m == null) {
            println("Manifest empty.");
            return;
        }

        dtm = currentProgram.getDataTypeManager();
        dtParser = new DataTypeParser(dtm, dtm, null, AllowedDataTypes.DYNAMIC);

        applyTypes(m);
        applyTypedGlobals(m);
        applyComments(m);
        createMissingFunctions(m);
        applySymbols(m);
        applyFunctions(m);
    }

    /**
     * On a blank Ghidra DB (fresh import + auto-analysis), many of our
     * function VAs aren't recognised as functions yet — auto-analysis
     * misses them, or they sit inside a region the analyser merged into
     * a single oversized body. Create the function entries here so the
     * subsequent symbol-rename and metadata-application passes have
     * something to bind to. No-op on an already-curated DB.
     */
    private void createMissingFunctions(Manifest m) {
        if (m.symbols == null || m.symbols.isEmpty()) return;
        long t0 = System.currentTimeMillis();
        int created = 0, skipped = 0, errors = 0;
        for (SymbolSpec se : m.symbols) {
            if (monitor.isCancelled()) break;
            if (se.kind == null || !"function".equals(se.kind)) continue;
            Address addr = parseAddr(se.va);
            Function existing = currentProgram.getFunctionManager().getFunctionAt(addr);
            if (existing != null) { skipped++; continue; }
            try {
                if (currentProgram.getListing().getInstructionAt(addr) == null) {
                    DisassembleCommand dis = new DisassembleCommand(
                        new AddressSet(addr), null, true);
                    dis.applyTo(currentProgram, monitor);
                }
                CreateFunctionCmd cmd = new CreateFunctionCmd(
                    null, addr, null, SourceType.USER_DEFINED);
                if (cmd.applyTo(currentProgram, monitor)) {
                    created++;
                } else {
                    errors++;
                    printerr("Create function at " + addr + ": "
                            + cmd.getStatusMsg());
                }
            } catch (Exception e) {
                errors++;
                printerr("Create function at " + addr + ": "
                        + e.getClass().getSimpleName() + ": " + e.getMessage());
            }
        }
        long dt = System.currentTimeMillis() - t0;
        println("Function creation: " + created + " created, " + skipped
                + " already-present, " + errors + " error(s) in " + dt + " ms.");
    }

    // ─── Manifest schema ─────────────────────────────────────────────────────

    private static class Manifest {
        Types types;
        @SerializedName("typed_globals") List<TypedGlobalSpec> typedGlobals;
        List<CommentSpec> comments;
        List<SymbolSpec> symbols;
        List<FunctionSpec> functions;
    }

    private static class Types {
        List<StructSpec> structs;
        List<StructSpec> unions;
        List<EnumSpec> enums;
        List<TypedefSpec> typedefs;
        @SerializedName("function_defs") List<FunctionDefSpec> functionDefs;
    }

    private static class StructSpec {
        String name;
        String namespace;
        int size;
        @SerializedName("plate_comment") String plateComment;
        List<FieldSpec> fields;
    }

    private static class FieldSpec {
        int offset;
        String name;
        String type;
        @SerializedName("type_namespace") String typeNamespace;
        Integer size;
        String comment;
    }

    private static class EnumSpec {
        String name;
        String namespace;
        int size;
        List<EnumValueSpec> values;
    }

    private static class EnumValueSpec {
        String name;
        long value;
    }

    private static class TypedefSpec {
        String name;
        String namespace;
        String target;
    }

    private static class FunctionDefSpec {
        String name;
        String namespace;
        String returns;
        List<FunctionDefParamSpec> params;
    }

    private static class FunctionDefParamSpec {
        String name;
        String type;
    }

    private static class TypedGlobalSpec {
        String va;
        String type;
    }

    private static class CommentSpec {
        String va;
        String kind;
        String text;
    }

    private static class SymbolSpec {
        String va;
        String name;
        String kind;
    }

    private static class FunctionSpec {
        String va;
        @SerializedName("calling_convention") String callingConvention;
        @SerializedName("no_return") boolean noReturn;
        @SerializedName("custom_storage") boolean customStorage;
        @SerializedName("plate_comment") String plateComment;
        @SerializedName("return_type") String returnType;
        List<ParamSpec> params;
    }

    private static class ParamSpec {
        String name;
        String type;
        String storage;
    }

    // ─── Types ───────────────────────────────────────────────────────────────

    private void applyTypes(Manifest m) {
        if (m.types == null) return;
        long t0 = System.currentTimeMillis();
        int newTypes = 0, updated = 0, skipped = 0, errors = 0;

        // Pass 1 — declare every type by name so cross-references resolve
        // during pass 2 (struct → struct *, etc.).
        if (m.types.typedefs != null) {
            for (TypedefSpec t : m.types.typedefs) {
                try {
                    DataType target = dtm.getDataType(new CategoryPath(nsOf(t.namespace)), t.name);
                    if (target == null) {
                        DataType ref = dtParser.parse(t.target);
                        TypedefDataType td = new TypedefDataType(
                            new CategoryPath(nsOf(t.namespace)), t.name, ref, dtm);
                        dtm.addDataType(td, DataTypeConflictHandler.KEEP_HANDLER);
                        newTypes++;
                    } else skipped++;
                } catch (Exception e) {
                    errors++;
                    printerr("Type typedef " + t.namespace + "/" + t.name
                            + ": " + e.getClass().getSimpleName() + ": " + e.getMessage());
                }
            }
        }
        if (m.types.enums != null) {
            for (EnumSpec e : m.types.enums) {
                try {
                    if (ensureEnum(e)) newTypes++; else updated++;
                } catch (Exception ex) {
                    errors++;
                    printerr("Type enum " + e.namespace + "/" + e.name
                            + ": " + ex.getClass().getSimpleName() + ": " + ex.getMessage());
                }
            }
        }
        if (m.types.structs != null) {
            for (StructSpec s : m.types.structs) {
                try {
                    if (ensureStructShell(s, false)) newTypes++; else skipped++;
                } catch (Exception e) {
                    errors++;
                    printerr("Type struct shell " + s.namespace + "/" + s.name
                            + ": " + e.getClass().getSimpleName() + ": " + e.getMessage());
                }
            }
        }
        if (m.types.unions != null) {
            for (StructSpec u : m.types.unions) {
                try {
                    if (ensureUnionShell(u)) newTypes++; else skipped++;
                } catch (Exception e) {
                    errors++;
                    printerr("Type union shell " + u.namespace + "/" + u.name
                            + ": " + e.getClass().getSimpleName() + ": " + e.getMessage());
                }
            }
        }
        if (m.types.functionDefs != null) {
            for (FunctionDefSpec fd : m.types.functionDefs) {
                try {
                    if (ensureFunctionDef(fd)) newTypes++; else skipped++;
                } catch (Exception ex) {
                    errors++;
                    printerr("Type function_def " + fd.namespace + "/" + fd.name
                            + ": " + ex.getClass().getSimpleName() + ": " + ex.getMessage());
                }
            }
        }

        // Pass 2 — populate struct and union members now that every named
        // type exists (so pointer-to-struct etc. resolves).
        if (m.types.structs != null) {
            for (StructSpec s : m.types.structs) {
                try {
                    if (populateStruct(s)) updated++;
                } catch (Exception e) {
                    errors++;
                    printerr("Type struct members " + s.namespace + "/" + s.name
                            + ": " + e.getClass().getSimpleName() + ": " + e.getMessage());
                }
            }
        }
        if (m.types.unions != null) {
            for (StructSpec u : m.types.unions) {
                try {
                    if (populateUnion(u)) updated++;
                } catch (Exception e) {
                    errors++;
                    printerr("Type union members " + u.namespace + "/" + u.name
                            + ": " + e.getClass().getSimpleName() + ": " + e.getMessage());
                }
            }
        }

        long dt = System.currentTimeMillis() - t0;
        println("Types: " + newTypes + " new, " + updated + " updated, "
                + skipped + " unchanged, " + errors + " error(s) in " + dt + " ms.");
    }

    /** Create the struct as a sized shell of undefined1 bytes; return true if newly created. */
    private boolean ensureStructShell(StructSpec s, boolean alwaysReplace) {
        CategoryPath path = new CategoryPath(nsOf(s.namespace));
        DataType existing = dtm.getDataType(path, s.name);
        if (existing instanceof Structure) {
            if (((Structure) existing).getLength() != s.size && s.size > 0) {
                // Resize: replace contents
                Structure es = (Structure) existing;
                es.deleteAll();
                es.growStructure(s.size);
            }
            return false;
        }
        ensureCategory(path);
        StructureDataType shell = new StructureDataType(path, s.name, s.size, dtm);
        if (s.plateComment != null) {
            shell.setDescription(s.plateComment);
        }
        dtm.addDataType(shell, DataTypeConflictHandler.KEEP_HANDLER);
        return true;
    }

    private boolean ensureUnionShell(StructSpec u) {
        CategoryPath path = new CategoryPath(nsOf(u.namespace));
        DataType existing = dtm.getDataType(path, u.name);
        if (existing instanceof Union) {
            return false;
        }
        ensureCategory(path);
        UnionDataType ut = new UnionDataType(path, u.name, dtm);
        if (u.plateComment != null) {
            ut.setDescription(u.plateComment);
        }
        dtm.addDataType(ut, DataTypeConflictHandler.KEEP_HANDLER);
        return true;
    }

    private boolean ensureEnum(EnumSpec e) {
        CategoryPath path = new CategoryPath(nsOf(e.namespace));
        DataType existing = dtm.getDataType(path, e.name);
        boolean isNew = false;
        EnumDataType en;
        if (existing instanceof ghidra.program.model.data.Enum) {
            en = (EnumDataType) existing;
            // wipe existing values to mirror manifest
            for (String existingName : ((ghidra.program.model.data.Enum) en).getNames()) {
                en.remove(existingName);
            }
        } else {
            ensureCategory(path);
            en = new EnumDataType(path, e.name, e.size, dtm);
            dtm.addDataType(en, DataTypeConflictHandler.KEEP_HANDLER);
            en = (EnumDataType) dtm.getDataType(path, e.name);
            isNew = true;
        }
        if (e.values != null) {
            for (EnumValueSpec v : e.values) {
                en.add(v.name, v.value);
            }
        }
        return isNew;
    }

    private boolean ensureFunctionDef(FunctionDefSpec fd) throws Exception {
        CategoryPath path = new CategoryPath(nsOf(fd.namespace));
        DataType existing = dtm.getDataType(path, fd.name);
        if (existing instanceof ghidra.program.model.data.FunctionDefinition) {
            return false; // leave existing alone for now
        }
        ensureCategory(path);
        FunctionDefinitionDataType fdt = new FunctionDefinitionDataType(path, fd.name, dtm);
        if (fd.returns != null) {
            fdt.setReturnType(dtParser.parse(fd.returns));
        }
        if (fd.params != null && !fd.params.isEmpty()) {
            ParameterDefinition[] params = new ParameterDefinition[fd.params.size()];
            for (int i = 0; i < fd.params.size(); i++) {
                FunctionDefParamSpec p = fd.params.get(i);
                params[i] = new ParameterDefinitionImpl(p.name, dtParser.parse(p.type), null);
            }
            fdt.setArguments(params);
        }
        dtm.addDataType(fdt, DataTypeConflictHandler.KEEP_HANDLER);
        return true;
    }

    /** Place every manifest field on the struct via replaceAtOffset. */
    private boolean populateStruct(StructSpec s) throws Exception {
        CategoryPath path = new CategoryPath(nsOf(s.namespace));
        DataType dt = dtm.getDataType(path, s.name);
        if (!(dt instanceof Structure)) return false;
        Structure struct = (Structure) dt;
        boolean changed = false;
        if (s.plateComment != null && !s.plateComment.equals(struct.getDescription())) {
            struct.setDescription(s.plateComment);
            changed = true;
        }
        if (s.fields == null) return changed;
        for (FieldSpec f : s.fields) {
            DataType memberDT = dtParser.parse(f.type);
            int size = (f.size != null) ? f.size : memberDT.getLength();
            if (size <= 0) {
                throw new IllegalArgumentException(
                    "field " + s.name + "." + f.name + " (" + f.type
                    + "): no size — set `size = 0xN` in TOML.");
            }
            if (f.offset + size > struct.getLength()) {
                int grow = (f.offset + size) - struct.getLength();
                struct.growStructure(grow);
                changed = true;
            }
            DataTypeComponent existing = struct.getComponentContaining(f.offset);
            boolean same = existing != null
                && existing.getOffset() == f.offset
                && existing.getLength() == size
                && existing.getDataType().isEquivalent(memberDT)
                && eq(existing.getFieldName(), f.name)
                && eq(existing.getComment(), f.comment);
            if (!same) {
                struct.replaceAtOffset(f.offset, memberDT, size, f.name, f.comment);
                changed = true;
            }
        }
        return changed;
    }

    private boolean populateUnion(StructSpec u) throws Exception {
        CategoryPath path = new CategoryPath(nsOf(u.namespace));
        DataType dt = dtm.getDataType(path, u.name);
        if (!(dt instanceof Union)) return false;
        Union union = (Union) dt;
        boolean changed = false;
        if (u.plateComment != null && !u.plateComment.equals(union.getDescription())) {
            union.setDescription(u.plateComment);
            changed = true;
        }
        if (u.fields == null) return changed;
        // Unions are member-ordered, not offset-ordered. Compare current members
        // to the manifest; if they differ, rebuild from scratch.
        boolean needsRebuild = union.getNumComponents() != u.fields.size();
        if (!needsRebuild) {
            for (int i = 0; i < u.fields.size(); i++) {
                DataTypeComponent comp = union.getComponent(i);
                FieldSpec f = u.fields.get(i);
                DataType memberDT = dtParser.parse(f.type);
                if (!comp.getDataType().isEquivalent(memberDT)
                        || !eq(comp.getFieldName(), f.name)
                        || !eq(comp.getComment(), f.comment)) {
                    needsRebuild = true;
                    break;
                }
            }
        }
        if (needsRebuild) {
            for (int i = union.getNumComponents() - 1; i >= 0; i--) {
                union.delete(i);
            }
            for (FieldSpec f : u.fields) {
                DataType memberDT = dtParser.parse(f.type);
                int size = (f.size != null) ? f.size : memberDT.getLength();
                if (size <= 0) {
                    throw new IllegalArgumentException(
                        "union " + u.name + "." + f.name + " (" + f.type
                        + "): no size");
                }
                union.add(memberDT, size, f.name, f.comment);
            }
            changed = true;
        }
        return changed;
    }

    private void ensureCategory(CategoryPath path) {
        Category cat = dtm.getCategory(path);
        if (cat == null) dtm.createCategory(path);
    }

    private static String nsOf(String namespace) {
        if (namespace == null || namespace.isEmpty() || namespace.equals("/")) {
            return "/";
        }
        return namespace;
    }

    private static boolean eq(String a, String b) {
        if (a == null) a = "";
        if (b == null) b = "";
        return a.equals(b);
    }

    // ─── Typed globals ───────────────────────────────────────────────────────

    private void applyTypedGlobals(Manifest m) {
        if (m.typedGlobals == null || m.typedGlobals.isEmpty()) return;
        Listing listing = currentProgram.getListing();
        long t0 = System.currentTimeMillis();
        int applied = 0, skipped = 0, errors = 0;
        for (TypedGlobalSpec g : m.typedGlobals) {
            if (monitor.isCancelled()) break;
            Address addr = parseAddr(g.va);
            try {
                DataType dt = dtParser.parse(g.type);
                ghidra.program.model.listing.Data data = listing.getDataAt(addr);
                if (data != null && data.getDataType().isEquivalent(dt)) {
                    skipped++;
                    continue;
                }
                listing.clearCodeUnits(addr, addr.add(dt.getLength() - 1L), false);
                listing.createData(addr, dt);
                applied++;
            } catch (Exception e) {
                errors++;
                printerr("Typed-global at " + addr + " (" + g.type
                        + "): " + e.getClass().getSimpleName() + ": " + e.getMessage());
            }
        }
        long dt = System.currentTimeMillis() - t0;
        println("Typed globals: " + applied + " applied, " + skipped
                + " unchanged, " + errors + " error(s) in " + dt + " ms.");
    }

    // ─── Comments ────────────────────────────────────────────────────────────

    private void applyComments(Manifest m) {
        if (m.comments == null || m.comments.isEmpty()) return;
        Listing listing = currentProgram.getListing();
        long t0 = System.currentTimeMillis();
        int applied = 0, skipped = 0, errors = 0;
        for (CommentSpec c : m.comments) {
            if (monitor.isCancelled()) break;
            Address addr = parseAddr(c.va);
            int type = commentTypeOf(c.kind);
            if (type < 0) {
                errors++;
                printerr("Unknown comment kind `" + c.kind + "` at " + addr);
                continue;
            }
            try {
                String current = listing.getComment(type, addr);
                if (c.text.equals(current)) {
                    skipped++;
                    continue;
                }
                listing.setComment(addr, type, c.text);
                applied++;
            } catch (Exception e) {
                errors++;
                printerr("Comment at " + addr + " (" + c.kind
                        + "): " + e.getClass().getSimpleName() + ": " + e.getMessage());
            }
        }
        long dt = System.currentTimeMillis() - t0;
        println("Comments: " + applied + " applied, " + skipped
                + " unchanged, " + errors + " error(s) in " + dt + " ms.");
    }

    private static int commentTypeOf(String kind) {
        if (kind == null) return -1;
        switch (kind) {
            case "plate":       return CodeUnit.PLATE_COMMENT;
            case "end-of-line": return CodeUnit.EOL_COMMENT;
            case "pre":         return CodeUnit.PRE_COMMENT;
            case "post":        return CodeUnit.POST_COMMENT;
            case "repeatable":  return CodeUnit.REPEATABLE_COMMENT;
            // "decompiler" comments are stored separately; closest XML
            // analogue is plate, matching how openwa-re-data degrades them.
            case "decompiler":  return CodeUnit.PLATE_COMMENT;
            default:            return -1;
        }
    }

    // ─── Symbols ─────────────────────────────────────────────────────────────

    private void applySymbols(Manifest m) {
        if (m.symbols == null || m.symbols.isEmpty()) return;
        SymbolTable symbolTable = currentProgram.getSymbolTable();
        long t0 = System.currentTimeMillis();
        int applied = 0, skipped = 0, errors = 0;
        for (SymbolSpec se : m.symbols) {
            if (monitor.isCancelled()) break;
            Address addr = parseAddr(se.va);
            try {
                Symbol primary = symbolTable.getPrimarySymbol(addr);
                if (primary != null && se.name.equals(primary.getName())
                        && primary.getParentNamespace().isGlobal()) {
                    skipped++;
                    continue;
                }
                Function func = currentProgram.getFunctionManager().getFunctionAt(addr);
                if (func != null) {
                    func.setName(se.name, SourceType.USER_DEFINED);
                    applied++;
                    continue;
                }
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
        println("Symbols: " + applied + " applied, " + skipped
                + " unchanged, " + errors + " error(s) in " + dt + " ms.");
    }

    // ─── Function metadata ───────────────────────────────────────────────────

    private void applyFunctions(Manifest m) {
        if (m.functions == null || m.functions.isEmpty()) return;
        long t0 = System.currentTimeMillis();
        int applied = 0, notFound = 0, errors = 0;
        for (FunctionSpec fe : m.functions) {
            if (monitor.isCancelled()) break;
            Address addr = parseAddr(fe.va);
            Function func = currentProgram.getFunctionManager().getFunctionAt(addr);
            if (func == null) {
                notFound++;
                continue;
            }
            try {
                applyOneFunction(fe, func);
                applied++;
            } catch (Exception e) {
                errors++;
                printerr("Function metadata at " + addr + ": "
                        + e.getClass().getSimpleName() + ": " + e.getMessage());
            }
        }
        long dt = System.currentTimeMillis() - t0;
        println("Functions: " + applied + " applied, "
                + notFound + " no-function VA, " + errors + " error(s) in " + dt + " ms.");
    }

    private void applyOneFunction(FunctionSpec fe, Function func) throws Exception {
        if (fe.plateComment != null) {
            func.setComment(fe.plateComment);
        }
        boolean hasParams = fe.params != null && !fe.params.isEmpty();
        boolean hasReturn = fe.returnType != null;
        // Mirror Ghidra's `Function.hasCustomVariableStorage()` flag (carried
        // through from the ReExport sidecar). When set, we keep every
        // `storage` string verbatim under CUSTOM_STORAGE. When not set, we
        // ignore `storage` strings entirely and let Ghidra recompute storage
        // from the calling convention — necessary because the export step
        // emits `this = ECX` even on plain `__thiscall` functions, which
        // would otherwise flip the function to CUSTOM_STORAGE and leave the
        // stack params UNASSIGNED.
        boolean anyCustomStorage = fe.customStorage;
        if (hasParams || hasReturn) {
            Variable returnVar = null;
            if (hasReturn) {
                DataType rt = dtParser.parse(fe.returnType);
                if (anyCustomStorage) {
                    // CUSTOM_STORAGE mode requires explicit storage on every
                    // slot. The TOML schema doesn't (yet) have a
                    // `return_storage` field; synthesise from the type using
                    // the x86 32-bit convention: void → VOID_STORAGE,
                    // ≤4 bytes → EAX, 8 bytes → EDX:EAX. Anything else stays
                    // UNASSIGNED (rare; add a real `return_storage` field
                    // if it shows up).
                    VariableStorage retStorage = synthesiseReturnStorage(rt, currentProgram);
                    returnVar = new ReturnParameterImpl(rt, retStorage, currentProgram);
                } else {
                    returnVar = new ReturnParameterImpl(rt, currentProgram);
                }
            }
            List<Variable> newParams = new ArrayList<>();
            if (hasParams) {
                // Running stack offset for synthesised storage on params under
                // CUSTOM_STORAGE that lack an explicit `storage` string. The
                // TOML schema lets `this=ECX` be the only annotated slot on a
                // __thiscall with custom storage; the stack params behind it
                // would otherwise come back UNASSIGNED. Register-stored params
                // don't advance the offset; explicit stack slots jump it past
                // their reserved space.
                int nextStackOffset = 0x4;
                for (ParamSpec p : fe.params) {
                    DataType dt = dtParser.parse(p.type);
                    ParameterImpl param;
                    if (anyCustomStorage) {
                        VariableStorage vs;
                        if (p.storage != null) {
                            vs = parseStorage(p.storage, dt.getLength(), currentProgram);
                            if (p.storage.startsWith("stack:")) {
                                int paramSize = (dt.getLength() + 3) & ~3;
                                nextStackOffset = Math.max(
                                    nextStackOffset, vs.getStackOffset() + paramSize);
                            }
                            // Register-stored params (this=ECX/ESI etc) leave
                            // nextStackOffset alone.
                        } else {
                            int size = dt.getLength() > 0 ? dt.getLength() : 4;
                            int aligned = (size + 3) & ~3;
                            vs = new VariableStorage(currentProgram, nextStackOffset, size);
                            nextStackOffset += aligned;
                        }
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

    // ─── Helpers ─────────────────────────────────────────────────────────────

    private VariableStorage synthesiseReturnStorage(DataType rt, Program program)
            throws Exception {
        if ("void".equals(rt.getName())) {
            return VariableStorage.VOID_STORAGE;
        }
        int size = rt.getLength();
        if (size <= 0) {
            return VariableStorage.UNASSIGNED_STORAGE;
        }
        if (size <= 4) {
            Register eax = program.getRegister("EAX");
            return eax != null
                ? new VariableStorage(program, eax)
                : VariableStorage.UNASSIGNED_STORAGE;
        }
        if (size == 8) {
            Register edx = program.getRegister("EDX");
            Register eax = program.getRegister("EAX");
            return (edx != null && eax != null)
                ? new VariableStorage(program, edx, eax)
                : VariableStorage.UNASSIGNED_STORAGE;
        }
        return VariableStorage.UNASSIGNED_STORAGE;
    }

    private Address parseAddr(String va) {
        long v = Long.parseLong(va.substring(2), 16);
        return currentProgram.getAddressFactory().getDefaultAddressSpace().getAddress(v);
    }

    /**
     * Parse a storage spec from the TOML side:
     *   "ECX"          → single register
     *   "EDX:EAX"      → multi-register split
     *   "stack:0x4"    → stack slot at offset 4, size from the datatype
     *   "stack:0x10:4" → stack slot at offset 16, explicit byte size
     */
    private VariableStorage parseStorage(String spec, int dtSize, Program program)
            throws Exception {
        if (spec.startsWith("stack:")) {
            String rest = spec.substring("stack:".length());
            int colon = rest.indexOf(':');
            int offset, size;
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
