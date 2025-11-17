use crate::message::ChatMessage;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;

pub const CHUNK_SIZE: usize = 8192;
pub const MAX_MESSAGE_SIZE: usize = 8192; // 8KB max message size

pub enum TcpMessageHandlerError {
    IoError(std::io::Error),
    Disconnect,
}

#[allow(async_fn_in_trait)]
pub trait TcpMessageHandler {
    fn get_stream(&mut self) -> &mut tokio::net::TcpStream;

    async fn send_message_chunked(&mut self, message: ChatMessage) -> Result<(), std::io::Error> {
        let message_bytes: Vec<u8> = message.into();

        // Validate message size to prevent integer overflow
        let msg_len = u16::try_from(message_bytes.len()).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "Message too large")
        })?;

        // Send the message length first
        self.get_stream().write_all(&msg_len.to_be_bytes()).await?;

        // Send the message in chunks
        let mut bytes_sent = 0;
        while bytes_sent < message_bytes.len() {
            let chunk_size = std::cmp::min(CHUNK_SIZE, message_bytes.len() - bytes_sent);
            let chunk = &message_bytes[bytes_sent..bytes_sent + chunk_size];

            self.get_stream().write_all(chunk).await?;
            bytes_sent += chunk_size;
        }

        self.get_stream().flush().await?;

        // Wait for OK response (2 bytes: "OK")
        let mut ok_response = [0u8; 2];
        self.get_stream().read_exact(&mut ok_response).await?;

        if &ok_response != b"OK" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Did not receive OK response from server",
            ));
        }

        Ok(())
    }

    async fn read_message_chunked(&mut self) -> Result<ChatMessage, TcpMessageHandlerError> {
        // Read the first 2 bytes to get the message length
        let mut len_bytes = [0u8; 2];
        self.get_stream()
            .read_exact(&mut len_bytes)
            .await
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    TcpMessageHandlerError::Disconnect
                } else {
                    TcpMessageHandlerError::IoError(e)
                }
            })?;

        let msg_len = u16::from_be_bytes(len_bytes) as usize;

        // Validate message size to prevent memory exhaustion attacks
        if msg_len > MAX_MESSAGE_SIZE {
            return Err(TcpMessageHandlerError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Message exceeds maximum size",
            )));
        }

        // Read the message in chunks to handle large messages
        let mut message_bytes = Vec::with_capacity(msg_len);
        let mut bytes_read = 0;

        while bytes_read < msg_len {
            let mut chunk = vec![0u8; std::cmp::min(CHUNK_SIZE, msg_len - bytes_read)];
            let n = self
                .get_stream()
                .read(&mut chunk)
                .await
                .map_err(TcpMessageHandlerError::IoError)?;

            if n == 0 {
                return Err(TcpMessageHandlerError::Disconnect);
            }

            message_bytes.extend_from_slice(&chunk[..n]);
            bytes_read += n;
        }

        // Send OK response to acknowledge receipt
        self.get_stream().write_all(b"OK").await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                TcpMessageHandlerError::Disconnect
            } else {
                TcpMessageHandlerError::IoError(e)
            }
        })?;
        self.get_stream().flush().await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                TcpMessageHandlerError::Disconnect
            } else {
                TcpMessageHandlerError::IoError(e)
            }
        })?;
        let message = ChatMessage::from(message_bytes);

        Ok(message)
    }
}
