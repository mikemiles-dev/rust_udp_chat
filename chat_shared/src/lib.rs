#[derive(Debug, Clone, Copy)]
pub enum MessageError {
    InvalidFormat,
    InvalidLength,
    UnknownType,
}

#[derive(Debug, Clone, Copy)]
pub enum MessageTypes {
    Acknowledge,
    ChatMessage,
    Join,
    Leave,
    UserRename,
}

impl TryFrom<u8> for MessageTypes {
    type Error = MessageError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MessageTypes::Acknowledge),
            1 => Ok(MessageTypes::ChatMessage),
            2 => Ok(MessageTypes::Join),
            3 => Ok(MessageTypes::Leave),
            4 => Ok(MessageTypes::UserRename),
            _ => Err(MessageError::UnknownType),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message {
    pub id: u8,
    pub msg_type: MessageTypes,
    pub length: usize,
    pub content: Option<String>,
}

impl Message {
    pub fn new(msg_type: MessageTypes, content: Option<String>, id: u8) -> Self {
        Message {
            id,
            msg_type,
            length: content.as_ref().map_or(0, |s| s.len()),
            content,
        }
    }

    pub fn get_content(&self) -> Result<String, std::str::Utf8Error> {
        std::str::from_utf8(self.content.as_ref().unwrap_or(&String::new()).as_bytes())
            .map(|s| s.to_string())
    }
}

/// Protocol is: [1 byte id][1 byte type][2 byte content len][content bytes]
impl TryFrom<Message> for Vec<u8> {
    type Error = MessageError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let mut buffer = Vec::with_capacity(1 + 1 + 2 + message.length);
        let message_length =
            u16::try_from(message.length).map_err(|_| MessageError::InvalidLength)?;
        buffer.push(message.id);
        buffer.push(match message.msg_type {
            MessageTypes::Acknowledge => 0,
            MessageTypes::ChatMessage => 1,
            MessageTypes::Join => 2,
            MessageTypes::Leave => 3,
            MessageTypes::UserRename => 4,
        });
        buffer.extend_from_slice(&message_length.to_be_bytes());
        if let Some(content) = message.content {
            buffer.extend_from_slice(content.as_bytes());
        }
        Ok(buffer)
    }
}

/// Protocol is: [1 byte id][1 byte type][2 byte content len][content bytes]
impl<'a> TryFrom<&'a [u8]> for Message {
    type Error = MessageError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() < 2 {
            return Err(MessageError::InvalidFormat);
        }
        let id = value[0];
        let msg_type = MessageTypes::try_from(value[1])?;
        let msg_len: u16 = value[2..4]
            .try_into()
            .map(u16::from_be_bytes)
            .map_err(|_| MessageError::InvalidFormat)?;
        let content = if msg_len > 0 {
            Some(String::from_utf8_lossy(&value[4..]).to_string())
        } else {
            None
        };
        Ok(Message {
            id,
            msg_type,
            length: content.as_ref().map_or(0, |s| s.len()),
            content,
        })
    }
}
