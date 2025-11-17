#[derive(Debug, Clone, Copy)]
pub enum MessageTypes {
    ChatMessage,
    Join,
    Leave,
    UserRename,
    ListUsers,
    Unknown(u8),
}

impl From<u8> for MessageTypes {
    fn from(value: u8) -> Self {
        match value {
            1 => MessageTypes::ChatMessage,
            2 => MessageTypes::Join,
            3 => MessageTypes::Leave,
            4 => MessageTypes::UserRename,
            5 => MessageTypes::ListUsers,
            other => MessageTypes::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    msg_len: u16,
    pub msg_type: MessageTypes,
    content: Option<Vec<u8>>,
}

impl ChatMessage {
    pub fn get_content(&self) -> Option<&[u8]> {
        self.content.as_deref()
    }

    pub fn content_as_string(&self) -> Option<String> {
        self.content
            .as_ref()
            .and_then(|data| String::from_utf8(data.clone()).ok())
    }
}

#[derive(Debug)]
pub enum ChatMessageError {
    InvalidFormat,
    InvalidLength,
}

impl ChatMessage {
    pub fn try_new(
        msg_type: MessageTypes,
        content: Option<Vec<u8>>,
    ) -> Result<Self, ChatMessageError> {
        let msg_len = match &content {
            Some(data) => data
                .len()
                .checked_add(3)
                .ok_or(ChatMessageError::InvalidLength)?,
            None => 1, // only msg_type byte
        };
        Ok(ChatMessage {
            msg_len: u16::try_from(msg_len).map_err(|_| ChatMessageError::InvalidLength)?,
            msg_type,
            content,
        })
    }
}

// Protocol: [msg_len (2 bytes)][msg_type (1 byte)] [content (msg_len - 2 bytes)]
impl From<Vec<u8>> for ChatMessage {
    fn from(buffer: Vec<u8>) -> Self {
        if buffer.is_empty() {
            return ChatMessage {
                msg_len: 0,
                msg_type: MessageTypes::Unknown(0),
                content: None,
            };
        }
        if buffer.len() < 3 {
            return ChatMessage {
                msg_len: 3,
                msg_type: MessageTypes::Unknown(0),
                content: None,
            };
        }
        let msg_len = u16::from_be_bytes([buffer[0], buffer[1]]);
        let msg_type = MessageTypes::from(buffer[2]);
        let content = if buffer.len() > 1 {
            Some(buffer[3..].to_vec())
        } else {
            None
        };

        ChatMessage {
            msg_len,
            msg_type,
            content,
        }
    }
}

impl From<ChatMessage> for Vec<u8> {
    fn from(message: ChatMessage) -> Self {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&message.msg_len.to_be_bytes());
        buffer.push(match message.msg_type {
            MessageTypes::ChatMessage => 1,
            MessageTypes::Join => 2,
            MessageTypes::Leave => 3,
            MessageTypes::UserRename => 4,
            MessageTypes::ListUsers => 5,
            MessageTypes::Unknown(val) => val,
        });
        if let Some(content) = message.content {
            buffer.extend_from_slice(&content);
        }
        buffer
    }
}
