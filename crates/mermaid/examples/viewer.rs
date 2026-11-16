//! A window showing the Mermaid viewer with its zoom controls.
//!
//! Run with `cargo run -p guiraffe-mermaid --example viewer`.

use gpui_kit::component::Sizable as _;
use gpui_kit::component::button::Button;
use gpui_kit::{
    App, AppContext as _, Bounds, ClipboardItem, Context, Entity, IntoElement, ParentElement as _,
    Point, Render, Styled as _, Window, WindowBounds, WindowOptions, div, px, size,
};
use guiraffe_core::{ColorName, GuiraffeTheme, Scheme, Size};
use guiraffe_mermaid::{MermaidView, MermaidViewState};

const SAMPLES: &[(&str, &str)] = &[
    (
        "Flowchart",
        "flowchart LR\n  Chat[Agent chat] --> Parse[Parse + layout]\n  Parse --> Cache{In cache?}\n  Cache -->|hit| Paint[Paint]\n  Cache -->|miss| Draw[Draw SVG] --> Paint",
    ),
    (
        "Sequence",
        "sequenceDiagram\n  participant U as User\n  participant A as Agent\n  participant M as Mermaid\n  U->>A: stream a fence\n  A->>M: render on a task\n  M-->>A: image\n  A-->>U: diagram",
    ),
    (
        "State",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Drawing: source changes\n  Drawing --> Idle: superseded\n  Drawing --> Failed: parse error\n  Failed --> Drawing: source fixed\n  Idle --> [*]",
    ),
    ("Broken", "flowchart LR\n  A --> \n  ???"),
];

struct Gallery {
    diagram: Entity<MermaidViewState>,
    sample: usize,
    dark: bool,
}

impl Gallery {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut gallery = Self {
            diagram: MermaidView::create_state("diagram", cx),
            sample: 0,
            dark: false,
        };
        gallery.show(cx);
        gallery
    }

    fn show(&mut self, cx: &mut Context<Self>) {
        let source = SAMPLES[self.sample].1;
        let diagram = self.diagram.clone();
        let dark = self.dark;
        diagram.update(cx, |state, cx| state.set_source(source.into(), dark, cx));
        cx.notify();
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        self.sample = index % SAMPLES.len();
        self.show(cx);
    }

    fn toggle_scheme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        guiraffe_core::set_scheme(
            if self.dark {
                Scheme::Dark
            } else {
                Scheme::Light
            },
            cx,
        );
        self.show(cx);
        window.refresh();
    }
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = GuiraffeTheme::global(cx);
        let surfaces = theme.surfaces();
        let state = self.diagram.clone();
        let status = match state.read(cx).status() {
            guiraffe_mermaid::view::Status::Empty => "no diagram".to_string(),
            guiraffe_mermaid::view::Status::Drawing => "drawing…".to_string(),
            guiraffe_mermaid::view::Status::Ready => "ready".to_string(),
            guiraffe_mermaid::view::Status::Failed(message) => message.clone(),
        };

        let controls = div()
            .flex()
            .items_center()
            .gap(px(Size::Sm.spacing()))
            .child(Button::new("prev").small().label("Previous").on_click(
                cx.listener(|this, _, _, cx| this.select(this.sample + SAMPLES.len() - 1, cx)),
            ))
            .child(
                Button::new("next")
                    .small()
                    .label("Next")
                    .on_click(cx.listener(|this, _, _, cx| this.select(this.sample + 1, cx))),
            )
            .child(
                Button::new("zoom-out")
                    .small()
                    .label("−")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagram.update(cx, |state, cx| state.zoom_out(cx))
                    })),
            )
            .child(Button::new("fit").small().label("Fit").on_click(
                cx.listener(|this, _, _, cx| this.diagram.update(cx, |state, cx| state.fit(cx))),
            ))
            .child(
                Button::new("actual")
                    .small()
                    .label("100%")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.diagram.update(cx, |state, cx| state.reset_zoom(cx))
                    })),
            )
            .child(Button::new("zoom-in").small().label("+").on_click(
                cx.listener(|this, _, _, cx| {
                    this.diagram.update(cx, |state, cx| state.zoom_in(cx))
                }),
            ))
            .child(
                Button::new("copy")
                    .small()
                    .label("Copy SVG")
                    .on_click(cx.listener(|this, _, _, cx| {
                        let svg = this.diagram.read(cx).svg().map(str::to_owned);
                        if let Some(svg) = svg {
                            cx.write_to_clipboard(ClipboardItem::new_string(svg));
                            this.diagram.update(cx, |state, cx| {
                                state.set_message(Some("SVG copied".into()), cx)
                            });
                        }
                    })),
            )
            .child(
                Button::new("theme")
                    .small()
                    .label(if self.dark { "Light" } else { "Dark" })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_scheme(window, cx))),
            );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(surfaces.body.to_hsla())
            .text_color(surfaces.text.to_hsla())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(Size::Md.spacing()))
                    .px(px(Size::Lg.spacing()))
                    .py(px(Size::Sm.spacing()))
                    .border_b(px(1.0))
                    .border_color(surfaces.border.to_hsla())
                    .child(
                        div()
                            .text_size(px(Size::Lg.font_size()))
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(SAMPLES[self.sample].0),
                    )
                    .child(div().flex_1())
                    .child(controls),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(Size::Lg.spacing()))
                    .child(MermaidView::new("diagram").state(&self.diagram)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(Size::Sm.spacing()))
                    .px(px(Size::Lg.spacing()))
                    .py(px(Size::Xs.spacing()))
                    .border_t(px(1.0))
                    .border_color(surfaces.border.to_hsla())
                    .text_size(px(11.0))
                    .text_color(surfaces.dimmed.to_hsla())
                    .child(div().size(px(6.0)).rounded_full().bg(
                        if matches!(
                            state.read(cx).status(),
                            guiraffe_mermaid::view::Status::Failed(_)
                        ) {
                            ColorName::Red.shade(6).to_hsla()
                        } else {
                            ColorName::Teal.shade(6).to_hsla()
                        },
                    ))
                    .child(status)
                    .child(div().flex_1())
                    .child("Drag to pan · scroll to zoom · double-click to fit"),
            )
    }
}

fn main() {
    gpui_kit::application().run(|cx: &mut App| {
        guiraffe_core::init(cx);
        let bounds = Bounds::new(Point::default(), size(px(1000.0), px(720.0)));
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(Gallery::new),
        )
        .expect("window opens");
    });
}
