#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MessageTypes {
    ChatMessage,
    Join,
    Leave,
    UserRename,
    ListUsers,
    DirectMessage,
    Error,
    RenameRequest,
    FileTransfer,         // File data being sent: recipient|sender|filename|data
    FileTransferAck,      // Acknowledgment that file was received
    FileTransferRequest,  // Request to send file: recipient|filename|filesize
    FileTransferResponse, // Response to request: sender|accepted (0/1)
    SetStatus,            // Set user's status message
    Ping,                 // Server heartbeat to check if client is alive
    Pong,                 // Client response to Ping
    VersionCheck,         // Client sends version to server on connection: version string
    VersionMismatch, // Server responds with mismatch error: client_version|server_version|readme_url
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
            6 => MessageTypes::DirectMessage,
            7 => MessageTypes::Error,
            8 => MessageTypes::RenameRequest,
            9 => MessageTypes::FileTransfer,
            10 => MessageTypes::FileTransferAck,
            11 => MessageTypes::FileTransferRequest,
            12 => MessageTypes::FileTransferResponse,
            13 => MessageTypes::SetStatus,
            14 => MessageTypes::Ping,
            15 => MessageTypes::Pong,
            16 => MessageTypes::VersionCheck,
            17 => MessageTypes::VersionMismatch,
            other => MessageTypes::Unknown(other),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    msg_len: u32,
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
                .checked_add(5) // 4 bytes for length + 1 byte for type
                .ok_or(ChatMessageError::InvalidLength)?,
            None => 5, // only msg_type byte + len (4 bytes)
        };
        Ok(ChatMessage {
            msg_len: u32::try_from(msg_len).map_err(|_| ChatMessageError::InvalidLength)?,
            msg_type,
            content,
        })
    }
}

// Protocol: [msg_len (4 bytes)][msg_type (1 byte)][content (msg_len - 5 bytes)]
impl From<Vec<u8>> for ChatMessage {
    fn from(buffer: Vec<u8>) -> Self {
        if buffer.is_empty() {
            return ChatMessage {
                msg_len: 5,
                msg_type: MessageTypes::Unknown(0),
                content: None,
            };
        }
        if buffer.len() < 5 {
            return ChatMessage {
                msg_len: 5,
                msg_type: MessageTypes::Unknown(0),
                content: None,
            };
        }
        let msg_len = u32::from_be_bytes([buffer[0], buffer[1], buffer[2], buffer[3]]);
        let msg_type = MessageTypes::from(buffer[4]);
        let content = if buffer.len() > 5 {
            Some(buffer[5..].to_vec())
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
            MessageTypes::DirectMessage => 6,
            MessageTypes::Error => 7,
            MessageTypes::RenameRequest => 8,
            MessageTypes::FileTransfer => 9,
            MessageTypes::FileTransferAck => 10,
            MessageTypes::FileTransferRequest => 11,
            MessageTypes::FileTransferResponse => 12,
            MessageTypes::SetStatus => 13,
            MessageTypes::Ping => 14,
            MessageTypes::Pong => 15,
            MessageTypes::VersionCheck => 16,
            MessageTypes::VersionMismatch => 17,
            MessageTypes::Unknown(val) => val,
        });
        if let Some(content) = message.content {
            buffer.extend_from_slice(&content);
        }
        buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation_valid() {
        let content = b"Hello, World!".to_vec();
        let msg = ChatMessage::try_new(MessageTypes::ChatMessage, Some(content.clone()));
        assert!(msg.is_ok());
        let msg = msg.unwrap();
        assert_eq!(msg.msg_type, MessageTypes::ChatMessage);
        assert_eq!(msg.content, Some(content));
    }

    #[test]
    fn test_message_creation_none_content() {
        let msg = ChatMessage::try_new(MessageTypes::ListUsers, None);
        assert!(msg.is_ok());
        let msg = msg.unwrap();
        assert_eq!(msg.msg_len, 5); // 4 bytes length + 1 byte type
        assert_eq!(msg.content, None);
    }

    #[test]
    fn test_message_serialization() {
        let content = b"Test".to_vec();
        let msg = ChatMessage::try_new(MessageTypes::ChatMessage, Some(content.clone())).unwrap();
        let serialized: Vec<u8> = msg.clone().into();

        // Check structure: [4 bytes len][1 byte type][content]
        assert_eq!(serialized.len(), 4 + 1 + content.len());
        assert_eq!(serialized[4], 1); // ChatMessage type
        assert_eq!(&serialized[5..], content.as_slice());
    }

    #[test]
    fn test_message_deserialization() {
        let mut buffer = vec![];
        buffer.extend_from_slice(&9u32.to_be_bytes()); // length (4 + 1 + 4 = 9)
        buffer.push(1); // ChatMessage type
        buffer.extend_from_slice(b"Test");

        let msg = ChatMessage::from(buffer);
        assert_eq!(msg.msg_type, MessageTypes::ChatMessage);
        assert_eq!(msg.content_as_string(), Some("Test".to_string()));
    }

    #[test]
    fn test_message_roundtrip() {
        let original_content = b"Hello, World!".to_vec();
        let original_msg =
            ChatMessage::try_new(MessageTypes::DirectMessage, Some(original_content.clone()))
                .unwrap();

        let serialized: Vec<u8> = original_msg.into();
        let deserialized = ChatMessage::from(serialized);

        assert_eq!(deserialized.msg_type, MessageTypes::DirectMessage);
        assert_eq!(deserialized.content, Some(original_content));
    }

    #[test]
    fn test_message_types_from_u8() {
        assert!(matches!(MessageTypes::from(1), MessageTypes::ChatMessage));
        assert!(matches!(MessageTypes::from(2), MessageTypes::Join));
        assert!(matches!(MessageTypes::from(3), MessageTypes::Leave));
        assert!(matches!(MessageTypes::from(4), MessageTypes::UserRename));
        assert!(matches!(MessageTypes::from(5), MessageTypes::ListUsers));
        assert!(matches!(MessageTypes::from(6), MessageTypes::DirectMessage));
        assert!(matches!(MessageTypes::from(7), MessageTypes::Error));
        assert!(matches!(MessageTypes::from(99), MessageTypes::Unknown(99)));
    }

    #[test]
    fn test_empty_buffer_deserialization() {
        let msg = ChatMessage::from(vec![]);
        assert_eq!(msg.msg_len, 5);
        assert!(matches!(msg.msg_type, MessageTypes::Unknown(0)));
        assert_eq!(msg.content, None);
    }

    #[test]
    fn test_short_buffer_deserialization() {
        let msg = ChatMessage::from(vec![0, 1]); // Too short
        assert_eq!(msg.msg_len, 5);
        assert!(matches!(msg.msg_type, MessageTypes::Unknown(0)));
    }

    #[test]
    fn test_content_as_string_valid_utf8() {
        let msg =
            ChatMessage::try_new(MessageTypes::ChatMessage, Some(b"Valid UTF-8".to_vec())).unwrap();
        assert_eq!(msg.content_as_string(), Some("Valid UTF-8".to_string()));
    }

    #[test]
    fn test_content_as_string_invalid_utf8() {
        let msg = ChatMessage::try_new(
            MessageTypes::ChatMessage,
            Some(vec![0xFF, 0xFE, 0xFD]), // Invalid UTF-8
        )
        .unwrap();
        assert_eq!(msg.content_as_string(), None);
    }
}
