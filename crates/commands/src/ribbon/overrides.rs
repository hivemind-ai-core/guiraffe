//! User overrides for shortcuts and ribbon chord keys, persisted as a small JSON file.
//!
//! Safety rules (from the prototype): ids are stable command and entry ids, never indexes;
//! a missing id inherits its default and `null` unbinds a shortcut; writes are atomic
//! (unique temporary file, sync, rename); a malformed file is never overwritten — loading
//! reports it and [`OverrideFile`] refuses to save until it has been fixed and reloaded.
use crate::{CommandId, Keybinding};
use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
};

/// Larger files are treated as malformed rather than read into memory.
pub const MAX_OVERRIDE_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overrides {
    /// Command id to shortcut; `None` (JSON `null`) unbinds.
    #[serde(default)]
    pub shortcuts: BTreeMap<CommandId, Option<String>>,
    /// Ribbon entry id to chord key.
    #[serde(default)]
    pub chords: BTreeMap<String, String>,
}

impl Overrides {
    pub fn parse(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".into())
    }
    /// Parse every shortcut first, so one bad entry rejects the whole set.
    pub fn shortcut_bindings(&self) -> Result<Vec<(CommandId, Option<Keybinding>)>, String> {
        self.shortcuts
            .iter()
            .map(|(id, binding)| {
                let parsed = binding
                    .as_deref()
                    .map(Keybinding::parse)
                    .transpose()
                    .map_err(|e| format!("{id}: {e}"))?;
                Ok((id.clone(), parsed))
            })
            .collect()
    }
}

/// What loading found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Loaded {
    /// No file yet: defaults apply and saving is allowed.
    Missing,
    Ok(Overrides),
    /// Unreadable, too large or invalid: defaults apply, the file is left untouched and
    /// saving is blocked until it is fixed and reloaded.
    Malformed(String),
}

/// One override file at an explicit path (no implicit locations).
#[derive(Debug, Clone)]
pub struct OverrideFile {
    path: PathBuf,
    blocked: Option<String>,
}

impl OverrideFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            blocked: None,
        }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Why saving is refused, if it is.
    pub fn blocked(&self) -> Option<&str> {
        self.blocked.as_deref()
    }
    pub fn load(&mut self) -> Loaded {
        let loaded = read(&self.path);
        self.blocked = match &loaded {
            Loaded::Malformed(reason) => Some(reason.clone()),
            _ => None,
        };
        loaded
    }
    /// Atomically replace the file. Refused while a malformed original is unresolved.
    pub fn save(&self, overrides: &Overrides) -> Result<(), String> {
        if let Some(reason) = &self.blocked {
            return Err(format!(
                "override file is malformed ({reason}); fix it and reload"
            ));
        }
        let json = overrides.to_json();
        if json.len() as u64 > MAX_OVERRIDE_BYTES {
            return Err("overrides exceed the size limit".into());
        }
        let dir = self.path.parent().unwrap_or_else(|| Path::new("."));
        let name = self
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("overrides.json");
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let temp = dir.join(format!(".{name}.{}.{stamp}.tmp", std::process::id()));
        let write = || -> std::io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)?;
            file.write_all(json.as_bytes())?;
            file.sync_all()?;
            std::fs::rename(&temp, &self.path)
        };
        write().map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            e.to_string()
        })
    }
}

fn read(path: &Path) -> Loaded {
    match std::fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => return Loaded::Malformed(e.to_string()),
        Ok(meta) if meta.len() > MAX_OVERRIDE_BYTES => {
            return Loaded::Malformed("file exceeds 64 KiB".into());
        }
        Ok(_) => {}
    }
    match std::fs::read_to_string(path) {
        Ok(text) => match Overrides::parse(&text) {
            Ok(overrides) => Loaded::Ok(overrides),
            Err(e) => Loaded::Malformed(e),
        },
        Err(e) => Loaded::Malformed(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        // Next to the test binary, inside the target dir (not the system temp dir).
        let base = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        let dir = base.join(format!("ribbon-overrides-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }

    #[test]
    fn overrides_round_trip_preserves_unbound_vs_default() {
        let mut overrides = Overrides::default();
        overrides
            .shortcuts
            .insert(CommandId::parse("edit").expect("id"), None);
        overrides.chords.insert("records-entry".into(), "!".into());
        let json = overrides.to_json();
        assert_eq!(Overrides::parse(&json), Ok(overrides));
        assert!(Overrides::parse(r#"{"unknown":1}"#).is_err());
        assert!(Overrides::parse(r#"{"shortcuts":{"Not Valid":null}}"#).is_err());
        let bad = Overrides::parse(r#"{"shortcuts":{"a":"ctrl+","b":"ctrl+s"}}"#).expect("json");
        assert!(bad.shortcut_bindings().is_err());
    }

    #[test]
    fn malformed_files_are_preserved_and_block_writes_until_fixed() {
        let dir = dir("malformed");
        let path = dir.join("keys.json");
        let mut file = OverrideFile::new(&path);
        assert_eq!(file.load(), Loaded::Missing);
        file.save(&Overrides::default()).expect("first save");
        assert_eq!(file.load(), Loaded::Ok(Overrides::default()));
        std::fs::write(&path, "{ not json").expect("corrupt");
        assert!(matches!(file.load(), Loaded::Malformed(_)));
        assert!(file.save(&Overrides::default()).is_err());
        assert_eq!(std::fs::read_to_string(&path).expect("kept"), "{ not json");
        std::fs::write(&path, "{}").expect("fix");
        assert_eq!(file.load(), Loaded::Ok(Overrides::default()));
        let mut overrides = Overrides::default();
        overrides.chords.insert("x".into(), "y".into());
        file.save(&overrides).expect("save after fix");
        assert_eq!(file.load(), Loaded::Ok(overrides));
        let leftovers = std::fs::read_dir(&dir)
            .expect("list")
            .filter(|e| e.as_ref().is_ok_and(|e| e.file_name() != "keys.json"))
            .count();
        assert_eq!(leftovers, 0, "temporary files are renamed away");
        std::fs::write(&path, vec![b' '; MAX_OVERRIDE_BYTES as usize + 1]).expect("big");
        assert!(matches!(file.load(), Loaded::Malformed(_)));
    }
}
