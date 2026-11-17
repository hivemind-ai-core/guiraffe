//! GUIraffe icons: the complete Lucide set (ISC licence, see `assets/LICENSE-lucide`) served
//! as a GPUI asset source.
//!
//! gpui-kit's `IconName` enum names every Lucide icon, but its asset bundle ships only about
//! 100 SVGs, so most names silently render blank. [`GuiraffeAssets`] serves every Lucide icon
//! and delegates everything else to gpui-kit's assets:
//!
//! ```ignore
//! gpui_kit::application().with_assets(guiraffe_icons::GuiraffeAssets).run(|cx| { /* … */ });
//! ```

use std::borrow::Cow;

use gpui_kit::{AssetSource, Result, SharedString};

include!(concat!(env!("OUT_DIR"), "/icons.rs"));

/// Lucide icons + gpui-kit's own assets.
#[derive(Clone, Copy, Debug, Default)]
pub struct GuiraffeAssets;

/// The embedded SVG for `path` (e.g. `icons/sparkles.svg`), if it is a Lucide icon.
pub fn icon(path: &str) -> Option<&'static [u8]> {
    ICONS
        .binary_search_by(|(p, _)| (*p).cmp(path))
        .ok()
        .map(|i| ICONS[i].1)
}

/// Number of embedded icons.
pub fn count() -> usize {
    ICONS.len()
}

impl AssetSource for GuiraffeAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match icon(path) {
            Some(bytes) => Ok(Some(Cow::Borrowed(bytes))),
            None => gpui_kit::assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> = gpui_kit::assets::Assets.list(path)?;
        out.extend(
            ICONS
                .iter()
                .filter(|(p, _)| p.starts_with(path))
                .map(|(p, _)| SharedString::from(*p)),
        );
        out.sort();
        out.dedup();
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_gpui_kit_lacks_are_served() {
        assert!(count() > 2000, "{}", count());
        for name in [
            "sparkles",
            "mail",
            "message-square",
            "boxes",
            "git-branch",
            "file-code",
            "binary",
        ] {
            let path = format!("icons/{name}.svg");
            assert!(
                GuiraffeAssets.load(&path).ok().flatten().is_some(),
                "{path}"
            );
        }
        assert!(
            GuiraffeAssets
                .load("icons/heart.svg")
                .ok()
                .flatten()
                .is_some(),
            "still serves bundled icons"
        );
        assert!(
            GuiraffeAssets
                .load("icons/definitely-not-an-icon.svg")
                .ok()
                .flatten()
                .is_none()
        );
    }

    #[test]
    fn every_name_in_gpui_kits_icon_enum_resolves() {
        // The enum's names are the Lucide set; spot-check across the alphabet.
        use gpui_kit::assets::IconName as I;
        for icon in [
            I::Activity,
            I::Bell,
            I::Calendar,
            I::Database,
            I::Frame,
            I::GitBranch,
            I::Map,
            I::Palette,
            I::Sparkles,
            I::Zap,
        ] {
            let path = icon.path();
            assert!(
                GuiraffeAssets.load(&path).ok().flatten().is_some(),
                "{path}"
            );
        }
    }

    #[test]
    fn table_is_sorted_for_binary_search() {
        assert!(ICONS.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
