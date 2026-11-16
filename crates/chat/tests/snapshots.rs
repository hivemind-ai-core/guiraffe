//! Appearance uses fully redrawn headless frames; render isolation is gated
//! separately in GPUI interaction tests with ordinary retained frames.
use gpui_kit::{
    AppContext, Context, Entity, HeadlessAppContext, IntoElement, Render, Window, px, size,
};
use guiraffe_chat::{Attachment, ChatState, Message, MessageId, Role, ToolCall, ToolStatus};
use guiraffe_core::{GuiraffeTheme, Scheme, apply};
use guiraffe_motion::MotionPrefs;
use guiraffe_test::snapshot::{SnapshotConfig, assert_snapshot};
use std::sync::Arc;
struct Scene(Entity<ChatState>);
impl Render for Scene {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        guiraffe_chat::Chat::new(&self.0)
    }
}
fn scene(window: &mut Window, cx: &mut gpui_kit::App) -> Entity<Scene> {
    let chat=cx.new(|cx| {
        let mut chat=ChatState::new(window,cx);
        let mut hello=Message::new(MessageId(1),Role::Assistant,"Ready when you are. What are we building today?");
        hello.timestamp="09:41".into();chat.push_message(hello,cx);
        let mut user=Message::new(MessageId(2),Role::User,"How should I stream an agent response without redrawing the conversation?");
        user.timestamp="09:42".into();chat.push_message(user,cx);
        let mut answer=Message::new(MessageId(3),Role::Assistant,concat!(
            "### Keep the update small\n",
            "Give each message its own retained view. Send **token batches** through a bounded queue, then append only to the active message.\n\n",
            "```rust\nlet (id, producer) = chat.begin_reply(cx);\nproducer.push(\"Hello, workspace!\".into())?;\n```\n\n",
            "Earlier messages stay cached. Scroll up any time; **Jump to latest** brings you back.",
        ));
        answer.timestamp="09:42".into();
        answer.reasoning="Inspect the current view boundaries, then choose the smallest unit of work.".into();
        answer.tools.push(ToolCall{id:"inspect".into(),name:"Inspect workspace".into(),status:ToolStatus::Succeeded,arguments:"crates/chat".into(),result:"Retained message views found.".into()});
        chat.push_message(answer,cx);
        chat.add_attachment(Attachment{id:"context".into(),name:"architecture.md".into()},cx);
        chat
    });
    cx.new(|_| Scene(chat))
}
#[test]
fn chat_light_and_dark() {
    let config = SnapshotConfig::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/snapshots"))
        .update_from_env();
    for scheme in [Scheme::Light, Scheme::Dark] {
        // Snapshotter currently drops its configured assets and draws only two
        // frames. Keep this crate-local driver until the shared helper can draw
        // forced asset-settled frames.
        let text = gpui_kit::platform::current_platform(true).text_system();
        let mut cx = HeadlessAppContext::with_platform(
            text,
            Arc::new(gpui_kit::assets::Assets),
            gpui_kit::platform::current_headless_renderer,
        );
        cx.update(|cx| {
            guiraffe_chat::init(cx);
            apply(
                GuiraffeTheme {
                    scheme,
                    ..Default::default()
                },
                cx,
            );
            MotionPrefs::set(
                MotionPrefs {
                    reduced: true,
                    ..Default::default()
                },
                cx,
            );
        });
        let window = cx
            .open_window(size(px(940.), px(960.)), scene)
            .expect("headless chat");
        for _ in 0..4 {
            cx.update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            })
            .expect("appearance frame");
            cx.run_until_parked();
        }
        let image = cx
            .capture_screenshot(window.into())
            .expect("headless screenshot");
        assert_snapshot(
            if scheme == Scheme::Light {
                "chat-light"
            } else {
                "chat-dark"
            },
            &image,
            &config,
        )
        .unwrap_or_else(|e| panic!("{e:#}"));
    }
}
