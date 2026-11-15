//! Persisted window layout: one JSON file mapping window keys to their last state.
//! Writes are atomic (temp file + rename); an unreadable file is set aside, never lost.

use std::{
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::placement::Rect;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedWindow {
    pub bounds: Rect,
    #[serde(default)]
    pub maximized: bool,
    #[serde(default)]
    pub fullscreen: bool,
    #[serde(default)]
    pub display: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Layout {
    pub version: u32,
    #[serde(default)]
    pub windows: BTreeMap<String, SavedWindow>,
}

#[derive(Debug, PartialEq)]
pub enum LoadOutcome {
    /// No file yet.
    Missing,
    Loaded(Layout),
    /// File existed but was unreadable/corrupt/newer; it was moved to `set_aside`.
    Recovered {
        set_aside: PathBuf,
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct LayoutStore {
    path: PathBuf,
}

impl LayoutStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> io::Result<LoadOutcome> {
        let bytes = match std::fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(LoadOutcome::Missing),
            Err(e) => return Err(e),
        };
        let reason = match serde_json::from_slice::<Layout>(&bytes) {
            Ok(l) if l.version <= FORMAT_VERSION => return Ok(LoadOutcome::Loaded(l)),
            Ok(l) => format!("written by newer format version {}", l.version),
            Err(e) => e.to_string(),
        };
        let set_aside = self.free_aside_path();
        std::fs::rename(&self.path, &set_aside)?;
        Ok(LoadOutcome::Recovered { set_aside, reason })
    }

    pub fn save(&self, layout: &Layout) -> io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut layout = layout.clone();
        layout.version = FORMAT_VERSION;
        let json = serde_json::to_vec_pretty(&layout).map_err(io::Error::other)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, &self.path)
    }

    fn free_aside_path(&self) -> PathBuf {
        (1u32..)
            .map(|n| self.path.with_extension(format!("corrupt-{n}.json")))
            .find(|p| !p.exists())
            .unwrap_or_else(|| self.path.with_extension("corrupt.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Layout {
        let mut l = Layout {
            version: FORMAT_VERSION,
            ..Default::default()
        };
        l.windows.insert(
            "main".into(),
            SavedWindow {
                bounds: Rect::new(1.0, 2.0, 800.0, 600.0),
                maximized: true,
                fullscreen: false,
                display: Some("d".into()),
            },
        );
        l
    }

    #[test]
    fn missing_then_roundtrip() {
        let dir = tempfile::tempdir().expect("tempdir");
        let s = LayoutStore::new(dir.path().join("sub/layout.json"));
        assert_eq!(s.load().expect("load"), LoadOutcome::Missing);
        s.save(&sample()).expect("save");
        assert_eq!(s.load().expect("load"), LoadOutcome::Loaded(sample()));
        assert!(!dir.path().join("sub/layout.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_is_set_aside_each_time_without_clobbering() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("layout.json");
        let s = LayoutStore::new(&p);
        for n in 1..=2 {
            std::fs::write(&p, b"{ not json").expect("write");
            match s.load().expect("load") {
                LoadOutcome::Recovered { set_aside, .. } => {
                    assert_eq!(
                        set_aside,
                        dir.path().join(format!("layout.corrupt-{n}.json"))
                    );
                    assert_eq!(std::fs::read(&set_aside).expect("read"), b"{ not json");
                }
                o => panic!("{o:?}"),
            }
            assert!(!p.exists());
        }
    }

    #[test]
    fn newer_version_is_set_aside() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("layout.json");
        std::fs::write(&p, br#"{"version":99,"windows":{}}"#).expect("write");
        assert!(
            matches!(LayoutStore::new(&p).load().expect("load"), LoadOutcome::Recovered { reason, .. } if reason.contains("99"))
        );
    }

    #[test]
    fn tolerates_missing_optional_fields() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path().join("layout.json");
        std::fs::write(
            &p,
            br#"{"version":1,"windows":{"a":{"bounds":{"x":0,"y":0,"w":10,"h":10}}}}"#,
        )
        .expect("write");
        let LoadOutcome::Loaded(l) = LayoutStore::new(&p).load().expect("load") else {
            panic!()
        };
        assert!(!l.windows["a"].maximized);
    }
}
