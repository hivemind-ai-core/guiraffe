//! CodeViewer embedded in Markdown using a shared extension registry.
use gpui_kit::component::text::{MarkdownExtensions, TextView, TextViewState};
use gpui_kit::{App, AppContext, Context, Entity, IntoElement, Render, Window};
use gpui_kit::{ParentElement, Styled, div};
struct Demo {
    text: Entity<TextViewState>,
    extensions: MarkdownExtensions,
}
impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = guiraffe_core::GuiraffeTheme::global(cx).surfaces();
        div()
            .size_full()
            .bg(s.body.to_hsla())
            .text_color(s.text.to_hsla())
            .child(TextView::new(&self.text).markdown_extensions(self.extensions.clone()))
    }
}
fn main() {
    gpui_kit::application().with_assets(gpui_kit::assets::Assets).run(|cx: &mut App| {
        guiraffe_code::init(cx);
        let opened = cx.open_window(Default::default(), |_, cx| {
            let text = cx.new(|cx| TextViewState::markdown(
                "# Shared code viewer\n\nThe fence below uses the standalone viewer.\n\n```rust\nfn main() {\n    println!(\"Hello from shared code\");\n}\n```\n", cx));
            let extensions = guiraffe_code::code_fences(MarkdownExtensions::default(), 16);
            cx.new(|_| Demo { text, extensions })
        });
        if let Err(error) = opened { eprintln!("{error}"); cx.quit(); }
    });
}
