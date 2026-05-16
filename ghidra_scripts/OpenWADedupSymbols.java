// One-shot symbol-dedup script.
//
// Reads a JSON action list: [{ "address": "0x...", "winner": "..." }, ...]
// For each entry:
//   - If the address has a Function: rename the function to `winner` via
//     Function.setName(winner, USER_DEFINED).
//   - Delete every USER_DEFINED symbol at the address whose name isn't `winner`.
//
// Result: each listed address ends up with exactly one USER_DEFINED name (the
// winner) and no PRIMARY="n" stragglers.
//
// @category OpenWA

import com.google.gson.Gson;
import com.google.gson.annotations.SerializedName;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionManager;
import ghidra.program.model.symbol.SourceType;
import ghidra.program.model.symbol.Symbol;
import ghidra.program.model.symbol.SymbolTable;
import java.io.File;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;

public class OpenWADedupSymbols extends GhidraScript {

    static class ActionFile {
        List<Action> actions;
    }

    static class Action {
        String address;
        String winner;
    }

    @Override
    public void run() throws Exception {
        String path;
        String[] args = getScriptArgs();
        if (args != null && args.length > 0) {
            path = args[0];
        } else {
            path = "C:/koodia/worms-re/OpenWA/.openwa/scratch/dedup_actions.json";
        }
        File file = new File(path);
        if (!file.isFile()) {
            printerr("Missing action list at " + file.getAbsolutePath());
            return;
        }

        ActionFile m = new Gson().fromJson(Files.readString(file.toPath()), ActionFile.class);
        if (m == null || m.actions == null) {
            printerr("Empty action list");
            return;
        }

        SymbolTable st = currentProgram.getSymbolTable();
        FunctionManager fm = currentProgram.getFunctionManager();

        int renamedFns = 0;
        int deletedSyms = 0;
        int skippedAlreadyOk = 0;
        List<String> errors = new ArrayList<>();

        for (Action a : m.actions) {
            try {
                long offset = Long.parseLong(a.address.replaceFirst("^0[xX]", ""), 16);
                Address addr = currentProgram.getAddressFactory()
                        .getDefaultAddressSpace().getAddress(offset);

                Function fn = fm.getFunctionAt(addr);

                // Find any existing symbol at this address with the winner's name.
                Symbol winnerSym = null;
                for (Symbol s : st.getSymbols(addr)) {
                    if (s.getName().equals(a.winner)) {
                        winnerSym = s;
                        break;
                    }
                }

                // 1. Make `winner` the primary symbol.
                if (fn != null) {
                    if (!fn.getName().equals(a.winner)) {
                        // If a colliding USER_DEFINED alias with the winner's name
                        // exists, delete it first so setName can succeed (otherwise
                        // Ghidra rejects with "A symbol named X already exists at
                        // this address"). Then rename the function to winner. This
                        // also strips any non-root namespace from the function (the
                        // MFC `CFile::Read` cases become root-namespace `CFile__Read`),
                        // which is what we want since our XML→TOML import filters
                        // namespaced primaries.
                        if (winnerSym != null) {
                            st.removeSymbolSpecial(winnerSym);
                        }
                        fn.setName(a.winner, SourceType.USER_DEFINED);
                        renamedFns++;
                    }
                } else {
                    // Non-function address (data label, e.g. vtable). Promote winner.
                    if (winnerSym != null && !winnerSym.isPrimary()) {
                        winnerSym.setPrimary();
                        renamedFns++;
                    } else if (winnerSym == null) {
                        errors.add("[" + a.address + "] no symbol named " + a.winner
                                + " found and address has no function — skipping");
                        continue;
                    }
                }

                // 2. Walk every symbol at the address; drop any USER_DEFINED
                //    that isn't `winner`.
                Symbol[] syms = st.getSymbols(addr);
                int dropAtThisAddr = 0;
                for (Symbol s : syms) {
                    if (s.getSource() != SourceType.USER_DEFINED) continue;
                    if (s.getName().equals(a.winner)) continue;
                    if (st.removeSymbolSpecial(s)) {
                        deletedSyms++;
                        dropAtThisAddr++;
                    } else {
                        errors.add("removeSymbolSpecial failed for " + s.getName()
                                + " @ " + addr);
                    }
                }

                if (fn != null && fn.getName().equals(a.winner) && dropAtThisAddr == 0) {
                    skippedAlreadyOk++;
                }
            } catch (Exception ex) {
                errors.add("[" + a.address + " → " + a.winner + "]: " + ex.getMessage());
            }
        }

        println("Renamed " + renamedFns + " function(s)");
        println("Deleted " + deletedSyms + " secondary symbol(s)");
        println("Already-OK: " + skippedAlreadyOk);
        if (!errors.isEmpty()) {
            printerr(errors.size() + " error(s):");
            for (String e : errors) printerr("  " + e);
        }
    }
}
