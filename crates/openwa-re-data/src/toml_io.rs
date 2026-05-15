//! Reading and merging `re/**/*.toml` into a single in-memory catalog.

use crate::model::*;
use crate::repo::enumerate_toml;
use anyhow::{Context, Result, bail};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Merged view of every TOML file under `re/`. Keyed for fast lookup and
/// duplicate detection on import.
#[derive(Debug, Default)]
pub struct Catalog {
    pub functions: HashMap<Va, OwnedEntry<Function>>,
    pub globals: HashMap<Va, OwnedEntry<Global>>,
    pub labels: HashMap<Va, OwnedEntry<Label>>,
    pub structs: HashMap<String, OwnedEntry<Struct>>,
    pub unions: HashMap<String, OwnedEntry<Union>>,
    pub enums: HashMap<String, OwnedEntry<Enum>>,
    pub typedefs: HashMap<String, OwnedEntry<Typedef>>,
    pub function_defs: HashMap<String, OwnedEntry<FunctionDef>>,
    /// Externally-defined type names (built-in DTM archives). Used by the
    /// validator to recognise legitimate type references.
    pub external_types: HashSet<String>,
}

/// An entry tagged with the file path it came from. Used for duplicate-error
/// messages: "VA 0x500 declared in `re/a.toml` and `re/b.toml`".
#[derive(Debug)]
pub struct OwnedEntry<T> {
    pub value: T,
    pub source: PathBuf,
}

impl Catalog {
    pub fn load_from(re_dir: &Path) -> Result<Self> {
        let paths = enumerate_toml(re_dir)?;
        let mut cat = Catalog::default();
        for p in &paths {
            cat.merge_file(p)
                .with_context(|| format!("loading {}", p.display()))?;
        }
        Ok(cat)
    }

    fn merge_file(&mut self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)?;
        let file: ReFile =
            toml::from_str(&text).with_context(|| format!("parsing TOML in {}", path.display()))?;

        for f in file.function {
            insert_va(&mut self.functions, f.va, f, path, "function")?;
        }
        for g in file.global {
            insert_va(&mut self.globals, g.va, g, path, "global")?;
        }
        for l in file.label {
            insert_va(&mut self.labels, l.va, l, path, "label")?;
        }
        for s in file.r#struct {
            insert_name(&mut self.structs, s.name.clone(), s, path, "struct")?;
        }
        for u in file.union {
            insert_name(&mut self.unions, u.name.clone(), u, path, "union")?;
        }
        for e in file.r#enum {
            insert_name(&mut self.enums, e.name.clone(), e, path, "enum")?;
        }
        for t in file.typedef {
            insert_name(&mut self.typedefs, t.name.clone(), t, path, "typedef")?;
        }
        for fd in file.function_def {
            insert_name(
                &mut self.function_defs,
                fd.name.clone(),
                fd,
                path,
                "function_def",
            )?;
        }
        self.external_types.extend(file.external_types);
        Ok(())
    }

    pub fn total_entries(&self) -> usize {
        self.functions.len()
            + self.globals.len()
            + self.labels.len()
            + self.structs.len()
            + self.unions.len()
            + self.enums.len()
            + self.typedefs.len()
            + self.function_defs.len()
    }
}

fn insert_va<T>(
    map: &mut HashMap<Va, OwnedEntry<T>>,
    va: Va,
    value: T,
    path: &Path,
    kind: &str,
) -> Result<()> {
    if let Some(existing) = map.get(&va) {
        bail!(
            "duplicate {kind} at VA 0x{va:08X}: declared in {} and {}",
            existing.source.display(),
            path.display()
        );
    }
    map.insert(
        va,
        OwnedEntry {
            value,
            source: path.to_path_buf(),
        },
    );
    Ok(())
}

fn insert_name<T>(
    map: &mut HashMap<String, OwnedEntry<T>>,
    name: String,
    value: T,
    path: &Path,
    kind: &str,
) -> Result<()> {
    if let Some(existing) = map.get(&name) {
        bail!(
            "duplicate {kind} `{name}`: declared in {} and {}",
            existing.source.display(),
            path.display()
        );
    }
    map.insert(
        name,
        OwnedEntry {
            value,
            source: path.to_path_buf(),
        },
    );
    Ok(())
}
