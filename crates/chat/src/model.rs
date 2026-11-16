//! Provider-independent chat data. No GPUI dependency.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub u64);
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role { User, Assistant }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment { pub id: String, pub name: String }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus { Running, Succeeded, Failed }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall { pub id: String, pub name: String, pub status: ToolStatus, pub arguments: String, pub result: String }
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub role: Role,
    pub text: String,
    /// Display label supplied by the host; the library does not assume a timezone.
    pub timestamp: String,
    pub attachments: Vec<Attachment>,
    pub tools: Vec<ToolCall>,
    pub reasoning: String,
}
impl Message {
    pub fn new(id: MessageId, role: Role, text: impl Into<String>) -> Self {
        Self {id, role, text: text.into(), timestamp: String::new(), attachments: vec![], tools: vec![], reasoning: String::new()}
    }
}
