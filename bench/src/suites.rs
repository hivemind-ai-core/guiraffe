//! Add new crate workloads here, then list them in `registered`.
//! The same functions feed the CLI, criterion and instruction-count benches.
use crate::Suite;
use guiraffe_core::{
    ColorName, Scheme, Variant,
    variant::{ResolveCtx, resolve},
};
use guiraffe_windows::placement::{Display, PlacementRules, Rect, cascade, place};
use std::hint::black_box;

/// Resolve every variant in both schemes, including color contrast work.
pub fn core_variants() {
    for scheme in [Scheme::Light, Scheme::Dark] {
        let ctx = black_box(ResolveCtx::new(scheme));
        for variant in [
            Variant::Filled,
            Variant::Light,
            Variant::Outline,
            Variant::Subtle,
            Variant::Transparent,
            Variant::White,
            Variant::Default,
        ] {
            black_box(resolve(
                &ctx,
                black_box(ColorName::Blue),
                black_box(variant),
            ));
        }
    }
}

/// Restore and cascade a window against two displays, including disconnected UUID recovery.
pub fn windows_placement() {
    let displays = [
        Display {
            uuid: None,
            bounds: Rect::new(0., 0., 1920., 1080.),
        },
        Display {
            uuid: None,
            bounds: Rect::new(1920., 0., 2560., 1440.),
        },
    ];
    let saved = black_box(Rect::new(1800., -50., 1000., 800.));
    let bounds = place(
        Some((saved, Some("disconnected"))),
        black_box(&displays),
        &PlacementRules::default(),
    );
    black_box(cascade(
        bounds,
        black_box(7),
        displays.get(1).map(|display| &display.bounds),
    ));
}

/// Shared registration table consumed by the CLI, criterion and iai-callgrind.
/// Add workloads here once; each tool expands its own registration from this list.
#[macro_export]
macro_rules! with_suites {
    ($callback:ident) => {
        $callback! {
            (core_variants, "core-variants"),
            (windows_placement, "windows-placement"),
        }
    };
}
macro_rules! make_registry {
    ($(($function:ident, $name:literal),)*) => {
        /// Registered headless workloads. Names are stable baseline filenames.
        pub fn registered() -> Vec<Suite> {
            vec![$(Suite { name: $name, run: $function }),*]
        }
    };
}
crate::with_suites!(make_registry);
