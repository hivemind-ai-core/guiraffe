//! Built-in document widgets, mapped onto GUIraffe components and GPUI Controls.
//! Spacing comes from the inherited style context (spec/14), not from the document.

use gpui_kit::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, component::button::Button, div, px,
};
use guiraffe_components::{
    Blockquote, Card, Highlight, KbdGroup, KbdSeparator, Mark, Paper, Pill, RingProgress, Text,
    Title,
};
use guiraffe_core::{Size, StyleContext, Variant};
use guiraffe_layout::{Align, Group, Justify, Stack};

use crate::widget::{NodeView, Registry, widget};

fn gap(n: &NodeView<'_>) -> f32 {
    StyleContext::current()
        .density
        .space(n.size(n.prop_index("gap")).unwrap_or(Size::Sm))
}

fn flow_gap(n: &NodeView<'_>) -> f32 {
    n.num(n.prop_index("gap-px"))
        .filter(|v| (0.0..=f64::from(f32::MAX)).contains(v))
        .map_or_else(|| gap(n), |v| v as f32)
}

fn flow_align(n: &NodeView<'_>, default: Align) -> Align {
    match n.prop(n.prop_index("align")).as_str() {
        Some("start") => Align::Start,
        Some("center") => Align::Center,
        Some("end") => Align::End,
        Some("stretch") => Align::Stretch,
        Some("baseline") => Align::Baseline,
        _ => default,
    }
}

fn flow_justify(n: &NodeView<'_>) -> Justify {
    match n.prop(n.prop_index("justify")).as_str() {
        Some("center") => Justify::Center,
        Some("end") => Justify::End,
        Some("between") => Justify::Between,
        Some("around") => Justify::Around,
        Some("evenly") => Justify::Evenly,
        _ => Justify::Start,
    }
}

/// Split children into rows of `cols` (the last row may be shorter).
fn chunk_rows(kids: Vec<AnyElement>, cols: usize) -> Vec<Vec<AnyElement>> {
    let mut rows = Vec::new();
    let mut row = Vec::with_capacity(cols);
    for k in kids {
        row.push(k);
        if row.len() == cols {
            rows.push(std::mem::replace(&mut row, Vec::with_capacity(cols)));
        }
    }
    if !row.is_empty() {
        rows.push(row);
    }
    rows
}

fn button(n: &NodeView<'_>) -> Button {
    use gpui_kit::component::{
        Disableable, FocusableExt, Icon, Placement, Selectable, Sizable,
        button::{ButtonRounded, ButtonVariant, ButtonVariants},
    };

    let prop = |name| n.prop(n.prop_index(name));
    let flag = |name| prop(name).as_bool().unwrap_or(false);
    let variant = match prop("variant").as_str() {
        Some("primary") => ButtonVariant::Primary,
        Some("secondary") => ButtonVariant::Secondary,
        Some("danger") => ButtonVariant::Danger,
        Some("info") => ButtonVariant::Info,
        Some("success") => ButtonVariant::Success,
        Some("warning") => ButtonVariant::Warning,
        Some("ghost") => ButtonVariant::Ghost,
        Some("link") => ButtonVariant::Link,
        Some("text") => ButtonVariant::Text,
        _ => ButtonVariant::Default,
    };
    let rounding = match prop("rounded").as_str() {
        Some("none") => ButtonRounded::None,
        Some("sm") => ButtonRounded::Small,
        Some("lg") => ButtonRounded::Large,
        _ => ButtonRounded::Medium,
    };
    let rounding = prop("rounding-px")
        .as_f64()
        .map_or(rounding, |width| px(width.max(0.0) as f32).into());
    let mut button = Button::new(n.element_id())
        .label(n.text(n.prop_index("label")))
        .with_variant(variant)
        .rounded(rounding)
        .disabled(flag("disabled"))
        .loading(flag("loading"))
        .selected(flag("selected"))
        .open(flag("open"))
        .dropdown_caret(flag("dropdown-caret"))
        .focus_ring(prop("focus-ring").as_bool().unwrap_or(true))
        .tab_stop(prop("tab-stop").as_bool().unwrap_or(true))
        .tab_index(prop("tab-index").as_f64().unwrap_or_default() as isize);
    if let Some(size) = n.size(n.prop_index("size")) {
        button = button.with_size(px(size.control_height()));
    }
    if flag("outline") {
        button = button.outline();
    }
    if flag("compact") {
        button = button.compact();
    }
    if let Some(toggled) = prop("toggled").as_bool() {
        button = button.toggled(toggled);
    }
    if let Some(path) = prop("icon").as_str() {
        button = button.icon(Icon::default().path(path.to_owned()));
    }
    if let Some(path) = prop("loading-icon").as_str() {
        button = button.loading_icon(Icon::default().path(path.to_owned()));
    }
    if let Some(role) = match prop("role").as_str() {
        Some("button") => Some(gpui_kit::Role::Button),
        Some("check-box") => Some(gpui_kit::Role::CheckBox),
        Some("radio-button") => Some(gpui_kit::Role::RadioButton),
        Some("switch") => Some(gpui_kit::Role::Switch),
        Some("menu-item") => Some(gpui_kit::Role::MenuItem),
        _ => None,
    } {
        button = button.role(role);
    }
    if let Some(id) = prop("accessibility-id").as_str() {
        button = button.accessibility_id(id.to_owned());
    }
    if let Some(label) = prop("accessibility-label").as_str() {
        button = button.accessibility_label(label.to_owned());
    }
    if let Some(tooltip) = prop("tooltip").as_str() {
        button = button.tooltip(tooltip.to_owned());
    }
    if let Some(placement) = match prop("tooltip-placement").as_str() {
        Some("top") => Some(Placement::Top),
        Some("bottom") => Some(Placement::Bottom),
        Some("left") => Some(Placement::Left),
        Some("right") => Some(Placement::Right),
        _ => None,
    } {
        button = button.tooltip_placement(placement);
    }
    if let Some(delay) = prop("tooltip-show-delay-ms").as_f64() {
        button = button.tooltip_show_delay(std::time::Duration::from_millis(
            delay.clamp(0.0, 60_000.0) as u64,
        ));
    }
    button
}

use crate::catalog::*;

pub fn register(r: &mut Registry) {
    crate::controls::register(r);
    r.register(
        STACK.0.id,
        widget(&STACK.1, |n, kids, _, _| {
            Stack::new()
                .gap_px(flow_gap(n))
                .align(flow_align(n, Align::Stretch))
                .justify(flow_justify(n))
                .grow(n.bool(n.prop_index("grow")))
                .children(kids)
                .into_any_element()
        }),
    );
    r.register(
        GROUP.0.id,
        widget(&GROUP.1, |n, kids, _, _| {
            Group::new()
                .gap_px(flow_gap(n))
                .align(flow_align(n, Align::Center))
                .justify(flow_justify(n))
                .grow(n.bool(n.prop_index("grow")))
                .wrap(n.prop(n.prop_index("wrap")).as_bool().unwrap_or(true))
                .children(kids)
                .into_any_element()
        }),
    );
    r.register(
        GRID.0.id,
        widget(&GRID.1, |n, kids, _, _| {
            let cols = n
                .num(n.prop_index("columns"))
                .unwrap_or(2.0)
                .clamp(1.0, 12.0) as usize;
            let g = gap(n);
            div()
                .flex()
                .flex_col()
                .gap(px(g))
                .children(chunk_rows(kids, cols).into_iter().map(|row| {
                    div()
                        .flex()
                        .gap(px(g))
                        .children(row.into_iter().map(|c| div().flex_1().min_w_0().child(c)))
                }))
                .into_any_element()
        }),
    );
    r.register(
        TITLE.0.id,
        widget(&TITLE.1, |n, _, _, _| {
            Title::new(n.text(n.prop_index("text")))
                .order(
                    n.num(n.prop_index("order"))
                        .map(|o| o.clamp(1.0, 6.0) as u8)
                        .unwrap_or(3),
                )
                .into_any_element()
        }),
    );
    r.register(
        TEXT.0.id,
        widget(&TEXT.1, |n, _, _, _| {
            let mut t =
                Text::new(n.text(n.prop_index("text"))).dimmed(n.bool(n.prop_index("dimmed")));
            if let Some(s) = n.size(n.prop_index("size")) {
                t = t.size(s);
            }
            if let Some(c) = n.color(n.prop_index("color")) {
                t = t.color(c);
            }
            if let Some(weight) = n.num(n.prop_index("font-weight")) {
                t = t.fw(FontWeight(weight.clamp(1.0, 1000.0) as f32));
            }
            if n.bool(n.prop_index("truncate")) {
                t = t.truncate(true);
            }
            if n.bool(n.prop_index("single-line")) {
                t = t
                    .truncate(true)
                    .h(px(Size::Xl.control_height()))
                    .line_height(px(Size::Xl.control_height()));
            }
            t.into_any_element()
        })
        .uniform_rows_if(|props| {
            matches!(
                TEXT.1
                    .prop_index("single-line")
                    .and_then(|index| props.get(index)),
                Some(crate::Prop::Lit(crate::Value::Bool(true)))
            )
        }),
    );
    r.register(
        HIGHLIGHT.0.id,
        widget(&HIGHLIGHT.1, |n, _, _, _| {
            Highlight::new(n.text(n.prop_index("text")))
                .term(n.text(n.prop_index("term")))
                .into_any_element()
        }),
    );
    r.register(
        MARK.0.id,
        widget(&MARK.1, |n, _, _, _| {
            Mark::new(n.text(n.prop_index("text"))).into_any_element()
        }),
    );
    r.register(
        BLOCKQUOTE.0.id,
        widget(&BLOCKQUOTE.1, |n, kids, _, _| {
            let mut quote = Blockquote::new().children(kids);
            if let Some(color) = n.color(n.prop_index("color")) {
                quote = quote.color(color);
            }
            if let Some(cite) = n.prop(n.prop_index("cite")).as_str() {
                quote = quote.cite(cite.to_owned());
            }
            quote.into_any_element()
        }),
    );
    r.register(
        PAPER.0.id,
        widget(&PAPER.1, |n, kids, _, _| {
            let mut paper = Paper::new()
                .with_border(n.bool(n.prop_index("with-border")))
                .children(kids);
            if let Some(shadow) = n.size(n.prop_index("shadow")) {
                paper = paper.shadow(shadow);
            }
            if let Some(radius) = n.size(n.prop_index("radius")) {
                paper = paper.radius(radius);
            }
            if let Some(padding) = n.size(n.prop_index("padding")) {
                paper = paper.p(padding);
            }
            if n.bool(n.prop_index("no-padding")) {
                paper = paper.p_0();
            }
            paper.into_any_element()
        }),
    );
    r.register(
        KBD_GROUP.0.id,
        widget(&KBD_GROUP.1, |n, _, _, _| {
            let keys = n.prop(n.prop_index("keys"));
            let keys = (0..keys.list_len().unwrap_or_default())
                .filter_map(|index| keys.list_get(index).and_then(crate::Value::as_str));
            let mut group = KbdGroup::new(keys.map(str::to_owned));
            if n.prop(n.prop_index("separator")).as_str() == Some("then") {
                group = group.separator(KbdSeparator::Then);
            }
            if let Some(size) = n.size(n.prop_index("size")) {
                group = group.size(size);
            }
            if let Some(color) = n.color(n.prop_index("color")) {
                group = group.color(color);
            }
            if let Some(variant) = match n.prop(n.prop_index("variant")).as_str() {
                Some("filled") => Some(Variant::Filled),
                Some("light") => Some(Variant::Light),
                Some("outline") => Some(Variant::Outline),
                Some("subtle") => Some(Variant::Subtle),
                Some("transparent") => Some(Variant::Transparent),
                Some("white") => Some(Variant::White),
                Some("default") => Some(Variant::Default),
                _ => None,
            } {
                group = group.variant(variant);
            }
            group.into_any_element()
        }),
    );
    r.register(
        PILL.0.id,
        widget(&PILL.1, |n, _, _, _| {
            let mut pill = Pill::new(n.element_id(), n.text(n.prop_index("label")));
            if let Some(size) = n.size(n.prop_index("size")) {
                pill = pill.size(size);
            }
            if let Some(remove) = n.handler(n.event_index("remove")) {
                let id = n.spec.id.clone().unwrap_or_default();
                pill = pill.on_remove(move |element, window, cx| {
                    let id = if id.is_empty() {
                        element.to_string().into()
                    } else {
                        id.clone()
                    };
                    remove(crate::Value::Str(id), window, cx);
                });
            }
            pill.into_any_element()
        }),
    );
    r.register(
        CARD.0.id,
        widget(&CARD.1, |n, kids, _, _| {
            let border = n
                .prop(n.prop_index("with-border"))
                .as_bool()
                .unwrap_or(true);
            let mut card = Card::new().with_border(border);
            if let Some(shadow) = n.size(n.prop_index("shadow")) {
                card = card.shadow(shadow);
            }
            if let Some(radius) = n.size(n.prop_index("radius")) {
                card = card.radius(radius);
            }
            if let Some(padding) = n.size(n.prop_index("padding")) {
                card = card.p(padding);
            }
            card.children(kids).into_any_element()
        }),
    );
    r.register(
        RING.0.id,
        widget(&RING.1, |n, _, _, _| {
            let mut ring = RingProgress::new()
                .diameter(n.num(n.prop_index("diameter")).unwrap_or(64.0) as f32)
                .thickness(n.num(n.prop_index("thickness")).unwrap_or(12.0) as f32)
                .label(n.text(n.prop_index("label")));
            let sections = n.prop(n.prop_index("sections"));
            if let Some(len) = sections.list_len().filter(|len| *len > 0) {
                let value_key = ["value".into()];
                let color_key = ["color".into()];
                for i in 0..len {
                    if let Some(section) = sections.list_get(i)
                        && let Some(value) = section.field(&value_key).as_f64()
                        && let Some(color) = section
                            .field(&color_key)
                            .as_str()
                            .and_then(guiraffe_core::ColorName::from_name)
                    {
                        ring = ring.section(value.clamp(0.0, 100.0) as f32, color);
                    }
                }
            } else {
                ring = ring.section(
                    n.num(n.prop_index("value")).unwrap_or(0.0) as f32,
                    n.color(n.prop_index("color")).unwrap_or_default(),
                );
            }
            ring.into_any_element()
        }),
    );
    r.register(
        BUTTON.0.id,
        widget(&BUTTON.1, |n, children, _, _| {
            let mut b = button(n).children(children);
            let hover = if n.bool(n.prop_index("disabled")) || n.bool(n.prop_index("loading")) {
                None
            } else {
                n.handler(n.event_index("hover"))
            };
            let hover = match hover {
                Some(hover) if n.prop(n.prop_index("tooltip")).as_str().is_none() => {
                    b = b.on_hover(move |entered, window, cx| hover((*entered).into(), window, cx));
                    None
                }
                hover => hover,
            };
            let b = match n.handler(n.event_index("click")) {
                Some(h) => b
                    .on_click(move |_, w, cx| h(crate::Value::Null, w, cx))
                    .into_any_element(),
                None => b.into_any_element(),
            };
            // Kit's managed tooltip owns the button's hover listener when present.
            let b = if let Some(hover) = hover {
                div()
                    .id((n.element_id(), "hover"))
                    .flex()
                    .on_hover(move |entered, window, cx| hover((*entered).into(), window, cx))
                    .child(b)
                    .into_any_element()
            } else {
                b
            };
            // Document ids double as test/automation selectors in `test-support` builds.
            #[cfg(feature = "test-support")]
            if let Some(id) = n.selector() {
                return div()
                    .debug_selector(move || id.to_string())
                    .child(b)
                    .into_any_element();
            }
            b
        }),
    );
    // Content-sized child (Mocket `width: "hug"`): opts out of the parent's cross-axis stretch.
    r.register(
        HUG.0.id,
        widget(&HUG.1, |_, kids, _, _| {
            div()
                .flex()
                .flex_col()
                .self_start()
                .children(kids)
                .into_any_element()
        }),
    );
    r.register(
        SPACER.0.id,
        widget(&SPACER.1, |_, _, _, _| div().flex_1().into_any_element()),
    );
}

pub use crate::catalog::KINDS;
