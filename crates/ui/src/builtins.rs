//! Built-in document widgets, mapped onto GUIraffe components and GPUI Controls.
//! Spacing comes from the inherited style context (spec/14), not from the document.

use gpui_kit::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, component::button::Button, div, px,
};
use guiraffe_components::{
    AvatarGroup, Blockquote, Card, CardSection, ColorSwatch, Highlight, Indicator,
    IndicatorPosition, KbdGroup, KbdSeparator, Mark, Paper, Person, Pill, RingProgress,
    ScopedButton, SwatchSelected, Text, Title,
};
use guiraffe_core::{ColorName, Rgb, Size, StyleContext, Variant};
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

fn component_variant(n: &NodeView<'_>) -> Option<Variant> {
    match n.prop(n.prop_index("variant")).as_str() {
        Some("filled") => Some(Variant::Filled),
        Some("light") => Some(Variant::Light),
        Some("outline") => Some(Variant::Outline),
        Some("subtle") => Some(Variant::Subtle),
        Some("transparent") => Some(Variant::Transparent),
        Some("white") => Some(Variant::White),
        Some("default") => Some(Variant::Default),
        _ => None,
    }
}

fn configure_action_icon(
    n: &NodeView<'_>,
    mut icon: guiraffe_components::ActionIcon,
) -> guiraffe_components::ActionIcon {
    icon = icon
        .disabled(n.bool(n.prop_index("disabled")))
        .loading(n.bool(n.prop_index("loading")));
    if let Some(label) = n.prop(n.prop_index("label")).as_str() {
        icon = icon.label(label.to_owned());
    }
    if let Some(color) = n.color(n.prop_index("color")) {
        icon = icon.color(color);
    }
    if let Some(size) = n.size(n.prop_index("size")) {
        icon = icon.size(size);
    }
    if let Some(variant) = component_variant(n) {
        icon = icon.variant(variant);
    }
    if let Some(focusable) = n.prop(n.prop_index("focusable")).as_bool() {
        icon = icon.focusable(focusable);
    }
    if let Some(handler) = n.handler(n.event_index("click")) {
        icon = icon.on_click(move |_, window, cx| handler(crate::Value::Null, window, cx));
    }
    icon
}

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
        INDICATOR.0.id,
        widget(&INDICATOR.1, |n, kids, _, _| {
            let position = match n.prop(n.prop_index("position")).as_str() {
                Some("top-start") => IndicatorPosition::TopStart,
                Some("bottom-start") => IndicatorPosition::BottomStart,
                Some("bottom-end") => IndicatorPosition::BottomEnd,
                _ => IndicatorPosition::TopEnd,
            };
            let mut indicator = Indicator::new(div().children(kids))
                .position(position)
                .disabled(n.bool(n.prop_index("disabled")));
            if let Some(label) = n.prop(n.prop_index("label")).as_str() {
                indicator = indicator.label(label.to_owned());
            }
            if let Some(color) = n.color(n.prop_index("color")) {
                indicator = indicator.color(color);
            }
            if let Some(size) = n.size(n.prop_index("size")) {
                indicator = indicator.size(size);
            }
            indicator.into_any_element()
        }),
    );
    r.register(
        COLOR_SWATCH.0.id,
        widget(&COLOR_SWATCH.1, |n, _, _, cx| {
            let id = n.element_id();
            let mut swatch = match n.num(n.prop_index("rgb")) {
                Some(rgb) => {
                    ColorSwatch::new(id, Rgb::hex(rgb.clamp(0.0, 0xff_ffff as f64) as u32))
                }
                None => ColorSwatch::from_color(
                    id,
                    n.color(n.prop_index("color")).unwrap_or(ColorName::Blue),
                    cx,
                ),
            };
            let selected = match n.prop(n.prop_index("selected")).as_str() {
                Some("check") => SwatchSelected::Check,
                Some("ring") => SwatchSelected::Ring,
                _ => SwatchSelected::None,
            };
            swatch = swatch
                .selected(selected)
                .disabled(n.bool(n.prop_index("disabled")));
            if let Some(label) = n.prop(n.prop_index("label")).as_str() {
                swatch = swatch.label(label.to_owned());
            }
            if let Some(size) = n.size(n.prop_index("size")) {
                swatch = swatch.size(size);
            }
            if let Some(focusable) = n.prop(n.prop_index("focusable")).as_bool() {
                swatch = swatch.focusable(focusable);
            }
            if let Some(click) = n.handler(n.event_index("click")) {
                swatch =
                    swatch.on_click(move |_, window, cx| click(crate::Value::Null, window, cx));
            }
            swatch.into_any_element()
        }),
    );
    r.register(
        AVATAR_GROUP.0.id,
        widget(&AVATAR_GROUP.1, |n, kids, _, _| {
            let data = n.prop(n.prop_index("people"));
            let name = ["name".into()];
            let image = ["image".into()];
            let people = (0..data.list_len().unwrap_or_default()).filter_map(|index| {
                let item = data.list_get(index)?;
                let mut person = Person::new(item.field(&name).as_str()?.to_owned());
                if let Some(image) = item.field(&image).as_str() {
                    person = person.image(image.to_owned());
                }
                Some(person)
            });
            let mut group = AvatarGroup::new(people).id(n.element_id().to_string());
            if let Some(limit) = n.num(n.prop_index("limit")) {
                group = group.limit(limit.clamp(0.0, f64::from(u32::MAX)) as usize);
            }
            if let Some(color) = n.color(n.prop_index("color")) {
                group = group.color(color);
            }
            if let Some(size) = n.size(n.prop_index("size")) {
                group = group.size(size);
            }
            if !kids.is_empty() {
                group = group.more(div().children(kids));
            }
            group.into_any_element()
        }),
    );
    r.register(
        LOADER.0.id,
        widget(&LOADER.1, |n, _, window, cx| {
            use guiraffe_components::{Loader, LoaderType};
            use std::time::Duration;
            let key = guiraffe_core::widget_catalog::WidgetNode::state_id(n);
            let loader = window
                .use_keyed_state(key, cx, |_, cx| Loader::new(cx))
                .read(cx)
                .clone();
            let kind = match n.prop(n.prop_index("kind")).as_str() {
                Some("bars") => LoaderType::Bars,
                Some("dots") => LoaderType::Dots,
                _ => LoaderType::Oval,
            };
            let count = n.num(n.prop_index("count")).unwrap_or(3.).clamp(1., 12.) as usize;
            let period = n
                .num(n.prop_index("period-ms"))
                .unwrap_or(1200.)
                .clamp(0., f64::from(u32::MAX)) as u64;
            let label = n.prop(n.prop_index("label")).as_str().unwrap_or("Loading");
            loader.update(cx, |loader, cx| {
                loader.set_kind(kind, cx);
                loader.set_count(count, cx);
                loader.set_period(Duration::from_millis(period), cx);
                loader.set_label(label.to_owned(), cx);
                loader.set_size(n.size(n.prop_index("size")), cx);
                loader.set_color(n.color(n.prop_index("color")), cx);
            });
            loader.into_any_element()
        }),
    );
    r.register(
        ACTIVITY_TIMELINE.0.id,
        widget(&ACTIVITY_TIMELINE.1, |n, _, _, _| {
            use guiraffe_components::{Timeline, TimelineItem};
            let values = n.prop(n.prop_index("items"));
            let title = ["title".into()];
            let description = ["description".into()];
            let color = ["color".into()];
            let bullet = ["bullet".into()];
            let items = (0..values.list_len().unwrap_or_default()).filter_map(|index| {
                let value = values.list_get(index)?;
                let mut item = TimelineItem::new(value.field(&title).as_str()?.to_owned());
                if let Some(description) = value.field(&description).as_str() {
                    item = item.description(description.to_owned());
                }
                if let Some(color) = value.field(&color).as_str().and_then(ColorName::from_name) {
                    item = item.color(color);
                }
                if let Some(bullet) = value.field(&bullet).as_str() {
                    item = item.bullet(bullet.to_owned());
                }
                Some(item)
            });
            let active = n
                .num(n.prop_index("active"))
                .unwrap_or_default()
                .clamp(0., f64::from(u32::MAX)) as usize;
            let mut timeline = Timeline::new(items).active(active);
            if let Some(color) = n.color(n.prop_index("color")) {
                timeline = timeline.color(color);
            }
            if let Some(size) = n.size(n.prop_index("size")) {
                timeline = timeline.size(size);
            }
            if let Some(diameter) = n.num(n.prop_index("bullet-size")) {
                timeline = timeline.bullet_size(diameter.clamp(1., f64::from(f32::MAX)) as f32);
            }
            if let Some(width) = n.num(n.prop_index("line-width")) {
                timeline = timeline.line_width(width.clamp(1., f64::from(f32::MAX)) as f32);
            }
            timeline.into_any_element()
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
            if let Some(variant) = component_variant(n) {
                group = group.variant(variant);
            }
            group.into_any_element()
        }),
    );
    r.register(
        THEME_ICON.0.id,
        widget(&THEME_ICON.1, |n, _, _, _| {
            let path = n
                .prop(n.prop_index("icon"))
                .as_str()
                .unwrap_or("icons/check.svg");
            let mut icon = guiraffe_components::ThemeIcon::from_asset(path.to_owned())
                .variant(component_variant(n).unwrap_or(Variant::Filled));
            if let Some(color) = n.color(n.prop_index("color")) {
                icon = icon.color(color);
            }
            if let Some(size) = n.size(n.prop_index("size")) {
                icon = icon.size(size);
            }
            if let Some(radius) = n.size(n.prop_index("radius")) {
                icon = icon.radius(radius);
            }
            icon.into_any_element()
        }),
    );
    r.register(
        ACTION_ICON.0.id,
        widget(&ACTION_ICON.1, |n, _, _, _| {
            let path = n
                .prop(n.prop_index("icon"))
                .as_str()
                .unwrap_or("icons/search.svg");
            let icon = guiraffe_components::ActionIcon::from_asset(n.element_id(), path.to_owned());
            configure_action_icon(n, icon).into_any_element()
        }),
    );
    r.register(
        CLOSE_BUTTON.0.id,
        widget(&CLOSE_BUTTON.1, |n, _, _, _| {
            configure_action_icon(n, guiraffe_components::close_button(n.element_id()))
                .into_any_element()
        }),
    );
    r.register(
        STEPPER.0.id,
        widget(&STEPPER.1, |n, _, _, _| {
            use guiraffe_components::{Step, Stepper};
            let values = n.prop(n.prop_index("steps"));
            let label = ["label".into()];
            let description = ["description".into()];
            let icon = ["icon".into()];
            let steps = (0..values.list_len().unwrap_or_default()).filter_map(|index| {
                let entry = values.list_get(index)?;
                let mut step = Step::new(entry.field(&label).as_str()?.to_owned());
                if let Some(description) = entry.field(&description).as_str() {
                    step = step.description(description.to_owned());
                }
                if let Some(icon) = entry.field(&icon).as_str() {
                    step = step.icon(gpui_kit::component::Icon::default().path(icon.to_owned()));
                }
                Some(step)
            });
            let active = n
                .num(n.prop_index("active"))
                .unwrap_or_default()
                .clamp(0., f64::from(u32::MAX)) as usize;
            let mut stepper = Stepper::new(steps).active(active);
            if let Some(size) = n.size(n.prop_index("size")) {
                stepper = stepper.size(size);
            }
            if let Some(color) = n.color(n.prop_index("color")) {
                stepper = stepper.color(color);
            }
            if let Some(focusable) = n.prop(n.prop_index("focusable")).as_bool() {
                stepper = stepper.focusable(focusable);
            }
            if let Some(handler) = n.handler(n.event_index("step-click")) {
                stepper = stepper.on_step_click(move |index, _, window, cx| {
                    handler(crate::Value::Num(index as f64), window, cx);
                });
            }
            stepper.into_any_element()
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
            let borders = n.prop(n.prop_index("section-borders"));
            for (index, child) in kids.into_iter().enumerate() {
                card = match borders.list_get(index).and_then(|value| value.as_bool()) {
                    Some(border) => {
                        card.section(CardSection::new().with_border(border).child(child))
                    }
                    None => card.child(child),
                };
            }
            card.into_any_element()
        }),
    );
    r.register(
        CARD_SECTION.0.id,
        widget(&CARD_SECTION.1, |n, kids, _, _| {
            CardSection::new()
                .with_border(n.bool(n.prop_index("with-border")))
                .children(kids)
                .into_any_element()
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
            if let Some(handler) = n.handler(n.event_index("click")) {
                b = b.on_click(move |_, window, cx| handler(crate::Value::Null, window, cx));
            }
            let b = if n.bool(n.prop_index("inherit-focus-scope")) {
                let mut scoped = ScopedButton::new(b);
                let tab_stop = n.prop_index("tab-stop");
                if guiraffe_core::widget_catalog::WidgetNode::is_set(n, tab_stop)
                    && let Some(focusable) = n.prop(tab_stop).as_bool()
                {
                    scoped = scoped.focusable(focusable);
                }
                scoped.into_any_element()
            } else {
                b.into_any_element()
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
