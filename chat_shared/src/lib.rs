#[derive(Debug, Clone, Copy)]
pub enum MessageError {
    InvalidFormat,
    InvalidLength,
    UnknownType,
}

#[derive(Debug, Clone, Copy)]
pub enum MessageTypes {
    ChatMessage,
    Join,
    Leave,
}

impl TryFrom<u8> for MessageTypes {
    type Error = MessageError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(MessageTypes::ChatMessage),
            1 => Ok(MessageTypes::Join),
            2 => Ok(MessageTypes::Leave),
            _ => Err(MessageError::UnknownType),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Message<'a> {
    pub id: u8,
    pub msg_type: MessageTypes,
    pub length: usize,
    pub content: &'a [u8],
}

impl<'a> Message<'a> {
    pub fn new(msg_type: MessageTypes, content: &'a [u8], id: u8) -> Self {
        Message {
            id,
            msg_type,
            length: content.len(),
            content,
        }
    }

    pub fn get_content(&self) -> Result<String, std::str::Utf8Error> {
        std::str::from_utf8(self.content).map(|s| s.to_string())
    }
}

/// Protocol is: [1 byte id][1 byte type][2 byte content len][content bytes]
impl<'a> TryFrom<Message<'a>> for Vec<u8> {
    type Error = MessageError;

    fn try_from(message: Message) -> Result<Self, Self::Error> {
        let mut buffer = Vec::with_capacity(1 + 1 + 2 + message.length);
        let message_length =
            u16::try_from(message.length).map_err(|_| MessageError::InvalidLength)?;
        buffer.push(message.id);
        buffer.push(match message.msg_type {
            MessageTypes::ChatMessage => 0,
            MessageTypes::Join => 1,
            MessageTypes::Leave => 2,
        });
        buffer.extend_from_slice(&message_length.to_be_bytes());
        buffer.extend_from_slice(message.content);
        Ok(buffer)
    }
}

impl<'a> TryFrom<&'a [u8]> for Message<'a> {
    type Error = MessageError;

    fn try_from(value: &'a [u8]) -> Result<Self, Self::Error> {
        if value.len() < 2 {
            return Err(MessageError::InvalidFormat);
        }
        let id = value[0];
        let msg_type = MessageTypes::try_from(value[1])?;
        let content = &value[2..];
        Ok(Message {
            id,
            msg_type,
            length: content.len(),
            content,
        })
    }
}
