use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::time::{Duration, Instant};

// We use tokio components for asynchronous networking and timing
use tokio::net::UdpSocket;

// --- CONFIGURATION ---
const MAX_PACKET_SIZE: usize = 1024;
const HEADER_SIZE: usize = 8; // 4 bytes for sequence, 4 for ack
const TIMEOUT_DURATION: Duration = Duration::from_millis(150);
const MAX_RETRIES: u8 = 5;

// --- 1. PACKET STRUCTURE ---

/// Represents a single packet used in our reliable protocol.
///
/// Format: [4 bytes SeqNum] [4 bytes AckNum] [Payload...]
#[derive(Debug, Clone)]
struct ReliablePacket {
    seq_num: u32,
    ack_num: u32,
    payload: Vec<u8>,
}

impl ReliablePacket {
    /// Creates a packet from a raw buffer.
    fn from_bytes(buffer: &[u8]) -> Option<Self> {
        if buffer.len() < HEADER_SIZE {
            return None;
        }

        // Deserialize header (little-endian assumed)
        let seq_bytes = <[u8; 4]>::try_from(&buffer[0..4]).ok()?;
        let ack_bytes = <[u8; 4]>::try_from(&buffer[4..8]).ok()?;

        let seq_num = u32::from_le_bytes(seq_bytes);
        let ack_num = u32::from_le_bytes(ack_bytes);

        // Copy payload
        let payload = buffer[HEADER_SIZE..].to_vec();

        Some(ReliablePacket {
            seq_num,
            ack_num,
            payload,
        })
    }

    /// Serializes the packet into a byte vector for sending.
    fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(HEADER_SIZE + self.payload.len());
        buffer.extend_from_slice(&self.seq_num.to_le_bytes());
        buffer.extend_from_slice(&self.ack_num.to_le_bytes());
        buffer.extend_from_slice(&self.payload);
        buffer
    }
}

// --- 2. WRAPPER STATE AND LOGIC ---

/// A simple reliable UDP wrapper.
pub struct UdpWrapper {
    socket: UdpSocket,

    // Sender state
    next_sequence_number: u32,

    // Receiver state
    expected_sequence_number: u32,

    // Reliability state: Tracks packets sent but not yet ACKed
    // (Packet, SentTime, Retries) - Note: In a production system, this would be a sliding window.
    unacknowledged_packets: HashMap<u32, (ReliablePacket, Instant, u8)>,
}

impl UdpWrapper {
    /// Binds to a local address and sets the target peer address asynchronously.
    pub async fn new(local_addr: &str) -> io::Result<Self> {
        // Use tokio's UdpSocket
        let socket = UdpSocket::bind(local_addr).await?;
        println!("Wrapper bound to: {}", local_addr);

        Ok(UdpWrapper {
            socket,
            next_sequence_number: 1,     // Start at 1
            expected_sequence_number: 1, // Start at 1
            unacknowledged_packets: HashMap::new(),
        })
    }

    /// Sends a message reliably, handling sequence numbers and retransmission using tokio::time::timeout.
    pub async fn send_message_reliable(
        &mut self,
        data: &[u8],
        peer_addr: SocketAddr,
    ) -> io::Result<()> {
        let seq = self.next_sequence_number;
        self.next_sequence_number += 1;

        println!("\n[SENDER] Preparing to send SEQ: {}", seq);

        let packet = ReliablePacket {
            seq_num: seq,
            // When sending data, the ack_num field contains the last expected sequence from the peer.
            ack_num: self.expected_sequence_number,
            payload: data.to_vec(),
        };

        let mut retries = 0;

        // Store the initial packet state for potential retransmission
        self.unacknowledged_packets
            .insert(seq, (packet.clone(), Instant::now(), retries));
        let packet_bytes = packet.to_bytes();

        // Initial send
        self.socket.send_to(&packet_bytes, peer_addr).await?;
        println!("[SENDER] Sent initial data packet SEQ: {}", seq);

        // The core retransmission/ACK loop
        loop {
            // 1. Check for max retries
            if retries >= MAX_RETRIES {
                return Err(io::Error::new(
                    ErrorKind::TimedOut,
                    format!(
                        "Failed to receive ACK for SEQ {} after {} retries.",
                        seq, MAX_RETRIES
                    ),
                ));
            }

            // 2. Wait for an acknowledgment (ACK) using a timeout
            let mut buf = [0; MAX_PACKET_SIZE];

            match tokio::time::timeout(TIMEOUT_DURATION, self.socket.recv_from(&mut buf)).await {
                // Received an ACK within the timeout period
                Ok(Ok((len, sender))) if sender == peer_addr => {
                    if let Some(ack_packet) = ReliablePacket::from_bytes(&buf[..len]) {
                        // Check if the received packet acknowledges our seq
                        if ack_packet.ack_num > seq {
                            println!("[SENDER] Received ACK for SEQ: {}", seq);
                            // Cleanup state and exit loop
                            self.unacknowledged_packets.remove(&seq);
                            self.expected_sequence_number = ack_packet.ack_num; // Update our receiver state
                            return Ok(());
                        }
                    }
                    // If it was a packet but not the right ACK, continue waiting
                }

                // Timeout occurred: Retransmit
                Err(_) => {
                    retries += 1;
                    self.socket.send_to(&packet_bytes, peer_addr).await?;
                    println!(
                        "[SENDER] Retransmitted data packet SEQ: {} (Attempt {})",
                        seq, retries
                    );
                }

                // Other I/O errors
                Ok(Err(e)) => return Err(e),

                // Ignore packets from unexpected senders
                _ => continue,
            }
        }
    }

    /// Listens for a single data packet and sends back an ACK if successful.
    pub async fn receive_data_and_ack(&mut self) -> io::Result<(Vec<u8>, SocketAddr)> {
        let mut buf = [0; MAX_PACKET_SIZE];

        loop {
            // 1. Try to receive a packet asynchronously
            let (len, sender) = match self.socket.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(e) => return Err(e),
            };

            // if sender != peer_addr {
            //     continue; // Ignore packets not from the expected peer
            // }

            if let Some(packet) = ReliablePacket::from_bytes(&buf[..len]) {
                let current_seq = packet.seq_num;

                if current_seq == self.expected_sequence_number {
                    // 2. Correct packet received! Process and ACK.
                    println!(
                        "[RECEIVER] Received correct data packet SEQ: {}",
                        current_seq
                    );

                    // 3. Send ACK for the NEXT expected sequence
                    let next_expected = current_seq + 1;
                    let ack_packet = ReliablePacket {
                        seq_num: 0,             // 0 for pure ACKs
                        ack_num: next_expected, // Acknowledging next expected packet
                        payload: Vec::new(),
                    };
                    let ack_bytes = ack_packet.to_bytes();

                    // Send ACK asynchronously
                    self.socket.send_to(&ack_bytes, sender).await?;

                    self.expected_sequence_number = next_expected;
                    println!(
                        "[RECEIVER] Sent ACK for next expected SEQ: {}",
                        next_expected
                    );

                    return Ok((packet.payload, sender));
                } else if current_seq < self.expected_sequence_number {
                    // 4. Duplicate/Out-of-Order packet received. Re-send previous ACK.
                    let re_ack = self.expected_sequence_number;
                    let ack_packet = ReliablePacket {
                        seq_num: 0,
                        ack_num: re_ack,
                        payload: Vec::new(),
                    };
                    let ack_bytes = ack_packet.to_bytes();
                    self.socket.send_to(&ack_bytes, sender).await?;
                    println!(
                        "[RECEIVER] Duplicate SEQ: {} received. Re-sent ACK for {}",
                        current_seq, re_ack
                    );
                } else {
                    // 5. Future packet received (missing intermediate packet). Drop or buffer (dropping for simplicity).
                    println!(
                        "[RECEIVER] Dropped future packet SEQ: {} (Expected {})",
                        current_seq, self.expected_sequence_number
                    );
                }
            }
        }
    }
}
