use gpui_kit::{
    AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, px, size,
};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_inputs::*;
use guiraffe_test::snapshot::{SnapshotConfig, Snapshotter, assert_snapshot};

struct Scene {
    chips: Entity<ChipGroup>,
    tags: Entity<TagsInput>,
    range: Entity<RangeSlider>,
    drop: Entity<Dropzone>,
}

impl Render for Scene {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .p(px(20.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(self.chips.clone())
            .child(self.tags.clone())
            .child(self.range.clone())
            .child(PasswordStrength::new("Correct-Horse-9"))
            .child(self.drop.clone())
    }
}

#[test]
fn inputs_light_and_dark() {
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for (scheme, name) in [
        (Scheme::Light, "inputs-light"),
        (Scheme::Dark, "inputs-dark"),
    ] {
        let mut snap = Snapshotter::new()
            .unwrap_or_else(|e| panic!("{e}"))
            .with_setup(move |cx| {
                guiraffe_inputs::init(cx);
                apply(
                    GuiraffeTheme {
                        scheme,
                        ..Default::default()
                    },
                    cx,
                );
            });
        let image = snap
            .render(size(px(520.), px(420.)), |window, cx| {
                let chips = ChipGroup::new(["React", "Svelte", "GPUI"], true, cx);
                chips.update(cx, |c, cx| {
                    c.toggle(0, cx);
                    c.toggle(2, cx);
                });
                let tags = TagsInput::new(TagRules::default(), "Add a tag…", window, cx);
                tags.update(cx, |t, cx| {
                    t.add("design", cx);
                    t.add("rust", cx);
                });
                let range = RangeSlider::new(Range2::new(0.0, 100.0, 1.0, 25.0, 70.0), cx);
                let drop = Dropzone::new(vec!["image/*"], "Drop images here", cx);
                cx.new(|_| Scene {
                    chips,
                    tags,
                    range,
                    drop,
                })
            })
            .unwrap_or_else(|e| panic!("{e:#}"));
        assert_snapshot(name, &image, &cfg).unwrap_or_else(|e| panic!("{e:#}"));
    }
}

struct FocusedRange(Entity<RangeSlider>);
impl Render for FocusedRange {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(GuiraffeTheme::global(cx).surfaces().body.to_hsla())
            .p(px(20.))
            .child(self.0.clone())
    }
}

#[test]
fn a_focused_range_keeps_a_solid_neutral_track_in_both_themes() {
    use gpui_kit::Focusable;
    let cfg = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        let theme = GuiraffeTheme {
            scheme,
            ..Default::default()
        };
        let neutral = theme.surfaces().track;
        let mut snap = Snapshotter::new()
            .expect("snapshotter")
            .with_setup(move |cx| {
                guiraffe_inputs::init(cx);
                apply(theme.clone(), cx);
            });
        let image = snap
            .render(size(px(400.), px(100.)), |window, cx| {
                let range = RangeSlider::new(Range2::new(0., 100., 1., 25., 70.), cx);
                range.read(cx).focus_handle(cx).focus(window, cx);
                cx.new(|_| FocusedRange(range))
            })
            .expect("focused slider");
        let rows = (0..image.height())
            .filter(|y| {
                image.get_pixel(80, *y).0[..3]
                    .iter()
                    .zip([neutral.r, neutral.g, neutral.b])
                    .all(|(a, b)| a.abs_diff(b) < 5)
            })
            .count();
        assert!(
            rows >= 7,
            "neutral track must remain a solid 4px rail at 2x scale, got {rows} rows"
        );
        assert_snapshot(
            if scheme == Scheme::Light {
                "range-focused-light"
            } else {
                "range-focused-dark"
            },
            &image,
            &cfg,
        )
        .expect("snapshot");
    }
}
