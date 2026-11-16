//! A simulated provider: the widget owns no transport or model-specific knowledge.
use gpui_kit::{AppContext, Context, Entity, IntoElement, Render, Subscription, Window};
use guiraffe_chat::{
    Attachment, Chat, ChatEvent, ChatState, Message, MessageId, Role, ToolCall, ToolStatus,
    stream::PushError,
};
use std::time::Duration;
pub struct ChatPage {
    chat: Entity<ChatState>,
    _events: Subscription,
}
impl ChatPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let chat=cx.new(|cx| {
            let mut chat=ChatState::new(window,cx);
            let mut message=Message::new(MessageId(1),Role::Assistant,"### A clearer next step\nAsk me about your workspace, review a change, or explore an idea. Try **/review** or add **@workspace** for context.");message.timestamp="Just now".into();chat.push_message(message,cx);chat
        });
        let events=cx.subscribe(&chat,|_,chat,event,cx|match event {
            ChatEvent::Submitted {..}|ChatEvent::Retry(_)=> {
                let (id,producer)=chat.update(cx,|chat,cx|chat.begin_reply(cx));
                chat.update(cx,|chat,cx|{chat.update_tool(id,tool(ToolStatus::Running),cx);});
                let executor=cx.background_executor().clone();let timers=executor.clone();
                executor.spawn(async move {
                    let response="Here’s a focused way forward.\n\n1. **Start with the behavior** you want to change.\n2. Keep updates local to the smallest view.\n3. Verify the result with a focused test.\n\n```rust\nlet (id, producer) = chat.begin_reply(cx);\nproducer.push(\"Small updates. Smooth UI.\".into())?;\n```\n\nYou can scroll up while I’m writing. I’ll keep your place.";
                    for chunk in response.split_inclusive(' ') {
                        let mut pending=chunk.to_owned();
                        loop {match producer.push(pending) {Ok(())=>break,Err(PushError::Full(value))=>{pending=value;timers.timer(Duration::from_millis(16)).await;},Err(_)=>return}}
                        timers.timer(Duration::from_millis(45)).await;
                    }
                }).detach();
            }
            ChatEvent::Finished(id)=>{chat.update(cx,|chat,cx|{chat.update_tool(*id,tool(ToolStatus::Succeeded),cx);});}
            ChatEvent::AttachmentRequested=>chat.update(cx,|chat,cx|chat.add_attachment(Attachment{id:"example".into(),name:"workspace-notes.md".into()},cx)),
            _=>{}
        });
        Self {
            chat,
            _events: events,
        }
    }
}
fn tool(status: ToolStatus) -> ToolCall {
    ToolCall {
        id: "inspect".into(),
        name: "Read workspace context".into(),
        status,
        arguments: "{ \"path\": \"src/\" }".into(),
        result: if status == ToolStatus::Running {
            "Reading…"
        } else {
            "Context ready · 12 source files"
        }
        .into(),
    }
}
impl Render for ChatPage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Chat::new(&self.chat)
    }
}
