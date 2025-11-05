#[derive(Debug, Clone, Copy)]
pub enum MessageTypes {
    ChatMessage,
    Join,
    Leave,
    UserRename,
    Unknown(u8),
}

impl From<u8> for MessageTypes {
    fn from(value: u8) -> Self {
        match value {
            1 => MessageTypes::ChatMessage,
            2 => MessageTypes::Join,
            3 => MessageTypes::Leave,
            4 => MessageTypes::UserRename,
            other => MessageTypes::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub msg_type: MessageTypes,
    pub content: Option<String>,
}

impl ChatMessage {
    pub fn get_content(&self) -> Option<&str> {
        self.content.as_deref()
    }
}

impl From<&[u8]> for ChatMessage {
    fn from(buffer: &[u8]) -> Self {
        if buffer.is_empty() {
            return ChatMessage {
                msg_type: MessageTypes::Unknown(0),
                content: None,
            };
        }

        let msg_type = MessageTypes::from(buffer[0]);
        let content = if buffer.len() > 1 {
            Some(String::from_utf8_lossy(&buffer[1..]).to_string())
        } else {
            None
        };

        ChatMessage { msg_type, content }
    }
}
