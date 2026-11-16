//! `cargo run -p guiraffe-chat --example agent_chat --features gpui-fast`
use gpui_kit::{
    AppContext, Context, Entity, IntoElement, Render, Subscription, Window, WindowOptions,
};
use guiraffe_chat::{Chat, ChatEvent, ChatState, Message, MessageId, Role, stream::PushError};
use std::time::Duration;
struct Example {
    chat: Entity<ChatState>,
    _events: Subscription,
}
impl Example {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat = cx.new(|cx| {
            let mut state = ChatState::new(window, cx);
            state.push_message(
                Message::new(
                    MessageId(1),
                    Role::Assistant,
                    "Hello! Try a message, a **/command**, or an **@mention**.",
                ),
                cx,
            );
            state
        });
        let events=cx.subscribe(&chat,|_,chat,event,cx| {
            if !matches!(event,ChatEvent::Submitted{..}|ChatEvent::Retry(_)) {return;}
            let (_,producer)=chat.update(cx,|chat,cx|chat.begin_reply(cx));
            let executor=cx.background_executor().clone();let timers=executor.clone();
            executor.spawn(async move {
                for token in "This reply comes from a background producer. The UI drains bounded token batches, while earlier messages stay retained.\n\n**Your provider goes here.**".split_inclusive(' ') {
                    let mut pending=token.to_owned();loop {match producer.push(pending) {Ok(())=>break,Err(PushError::Full(value))=>{pending=value;timers.timer(Duration::from_millis(16)).await;},Err(_)=>return}}
                    timers.timer(Duration::from_millis(45)).await;
                }
            }).detach();
        });
        Self {
            chat,
            _events: events,
        }
    }
}
impl Render for Example {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Chat::new(&self.chat)
    }
}
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            guiraffe_chat::init(cx);
            if let Err(error) = cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| Example::new(window, cx));
                guiraffe_overlays::root(view, window, cx)
            }) {
                eprintln!("Cannot open agent chat: {error}");
                cx.quit();
            }
        });
}
