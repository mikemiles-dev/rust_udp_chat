use std::collections::{HashMap, VecDeque};
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

// --- TOKIO IMPORTS ---
use tokio::net::UdpSocket;
use tokio::sync::Mutex;
use tokio::time::Instant; // Using Tokio's Instant

// --- CONSTANTS ---

/// The timeout duration before a packet is considered lost and needs retransmission.
const RTO: Duration = Duration::from_millis(500);

/// Maximum payload size (arbitrary choice, depends on network MTU).
const MAX_PAYLOAD_SIZE: usize = 1024;

/// Size of the header (SeqNum + AckNum + Flags) in bytes.
const HEADER_SIZE: usize = 10; // Updated size (u32 + u32 + u16)

/// Control flag indicating the packet is an Acknowledgement.
const FLAG_ACK: u16 = 0b0000_0001;

// --- PACKET STRUCTURE ---

/// Defines the structure of our RUDP packet header.
#[derive(Debug, Clone, Copy)]
// Note: We avoid #[repr(packed)] in async context and rely on manual serialization.
struct PacketHeader {
    /// Sequence number of this packet. Used for ordering and reliability.
    seq_num: u32,
    /// Acknowledgment number. Indicates the next sequential packet the sender expects to receive.
    ack_num: u32,
    /// Control flags (e.g., 1=ACK, 2=SYN, 4=FIN).
    flags: u16,
}

/// A full RUDP packet, combining the header and the payload.
struct Packet {
    header: PacketHeader,
    payload: Vec<u8>,
}

impl Packet {
    /// Converts a Packet struct into a byte buffer for transmission.
    fn to_bytes(&self) -> Vec<u8> {
        let mut buffer = Vec::new();

        // Convert u32s to network byte order (Big Endian) and append
        buffer.extend_from_slice(&self.header.seq_num.to_be_bytes());
        buffer.extend_from_slice(&self.header.ack_num.to_be_bytes());
        buffer.extend_from_slice(&self.header.flags.to_be_bytes());

        // Append the actual data
        buffer.extend_from_slice(&self.payload);
        buffer
    }

    /// Tries to convert a byte buffer back into a Packet.
    fn from_bytes(buffer: &[u8]) -> Option<Packet> {
        if buffer.len() < HEADER_SIZE {
            return None; // Buffer is too small to even contain the header
        }

        // Read header fields from Big Endian bytes
        let seq_num = u32::from_be_bytes(buffer[0..4].try_into().unwrap());
        let ack_num = u32::from_be_bytes(buffer[4..8].try_into().unwrap());
        let flags = u16::from_be_bytes(buffer[8..10].try_into().unwrap());

        // The rest is the payload
        let payload = buffer[HEADER_SIZE..].to_vec();

        Some(Packet {
            header: PacketHeader {
                seq_num,
                ack_num,
                flags,
            },
            payload,
        })
    }
}

// --- PEER STATE MANAGEMENT ---

/// A temporary structure to hold a packet waiting for acknowledgement.
#[derive(Debug, Clone)]
struct UnackedPacket {
    packet_bytes: Vec<u8>,
    /// The sequence number of the packet being sent.
    seq_num: u32,
    /// When this packet was last sent. Used to check for retransmission.
    last_sent: Instant,
}

/// Holds the necessary state for a single, reliable connection to one peer.
struct PeerState {
    // Outgoing state
    /// Next sequence number to be assigned to an outgoing packet.
    next_seq_num: u32,
    /// Packets that have been sent but not yet acknowledged (our retransmission queue).
    unacked_queue: VecDeque<UnackedPacket>,

    // Incoming state
    /// Next sequence number we EXPECT to receive from the peer. This is the ACK number we send.
    next_expected_seq_num: u32,
    /// Out-of-order packets received, waiting for the gap to be filled.
    reorder_buffer: HashMap<u32, Vec<u8>>,
    /// A buffer of reliable messages that have been fully ordered and are ready for the chat application to consume.
    received_messages: VecDeque<Vec<u8>>,

    /// Last time we had any activity with this peer (used for connection timeout/cleanup).
    last_activity: Instant,
}

impl PeerState {
    fn new(initial_seq: u32) -> Self {
        PeerState {
            next_seq_num: initial_seq,
            next_expected_seq_num: initial_seq,
            unacked_queue: VecDeque::new(),
            reorder_buffer: HashMap::new(),
            received_messages: VecDeque::new(),
            last_activity: Instant::now(),
        }
    }
}

// --- UDP WRAPPER ---

/// The main structure acting as the lightweight TCP layer.
pub struct UdpWrapper {
    /// The underlying standard UDP socket, shared across async tasks.
    socket: Arc<UdpSocket>,
    /// Shared state map storing connection details for all peers. Uses Tokio Mutex.
    peers: Arc<Mutex<HashMap<SocketAddr, PeerState>>>,
}

impl UdpWrapper {
    /// Creates a new UdpWrapper, binding the underlying socket to the given address.
    pub fn new(bind_addr: &str) -> io::Result<Arc<Self>> {
        // 1. Bind the standard blocking UDP socket
        let socket = std::net::UdpSocket::bind(bind_addr)?;

        // 2. EXPLICITLY set the socket to non-blocking mode.
        // This is necessary in some environments before converting to a Tokio socket.
        socket.set_nonblocking(true)?;

        // 3. Convert the non-blocking standard socket into a Tokio async socket
        let tokio_socket = UdpSocket::from_std(socket)?;

        let wrapper = UdpWrapper {
            socket: Arc::new(tokio_socket),
            peers: Arc::new(Mutex::new(HashMap::new())),
        };

        Ok(Arc::new(wrapper))
    }

    /// Utility function for the main simulation loop to find a connected peer's address.
    /// This resolves the issue where the client's actual sending port might be ephemeral.
    pub async fn get_first_peer_addr(&self) -> Option<SocketAddr> {
        let peers = self.peers.lock().await;
        peers.keys().next().cloned()
    }

    /// --- Outgoing Reliability: Sending and Retransmitting ---

    /// Sends raw data to a target address reliably. This is now an async function.
    pub async fn send_data(&self, target_addr: SocketAddr, data: Vec<u8>) -> io::Result<()> {
        let mut peers = self.peers.lock().await;
        // The Mutex is unlocked when the function returns or the lock guard goes out of scope.
        let peer_state = peers
            .entry(target_addr)
            .or_insert_with(|| PeerState::new(1));

        // 1. Determine the sequence and ACK number for this outgoing packet.
        let seq_num = peer_state.next_seq_num;
        let ack_num = peer_state.next_expected_seq_num;
        peer_state.next_seq_num = peer_state.next_seq_num.wrapping_add(1);

        let packet = Packet {
            header: PacketHeader {
                seq_num,
                ack_num,
                flags: 0b0000_0000, // No special flags for data
            },
            payload: data,
        };

        let packet_bytes = packet.to_bytes();

        // 2. Queue the packet for retransmission (it hasn't been ACKed yet).
        peer_state.unacked_queue.push_back(UnackedPacket {
            packet_bytes: packet_bytes.clone(),
            seq_num,
            last_sent: Instant::now(),
        });

        // 3. Send the packet over the unreliable UDP socket.
        self.socket
            .send_to(&packet_bytes, target_addr)
            .await
            .map(|_| ())
    }

    /// Periodically checks the unacked queue and retransmits lost packets. This is now an async function.
    pub async fn handle_retransmissions(&self) {
        let mut peers = self.peers.lock().await;
        let now = Instant::now();

        // Iterate over all active peer connections
        for (addr, state) in peers.iter_mut() {
            let socket = &self.socket;

            // Collect packets to retransmit
            let mut packets_to_send = Vec::new();
            for unacked in state.unacked_queue.iter_mut() {
                if now.duration_since(unacked.last_sent) > RTO {
                    // Timeout occurred, mark for retransmission
                    packets_to_send.push((unacked.packet_bytes.clone(), *addr, unacked.seq_num));
                    // Update the last sent time to reset the timer
                    unacked.last_sent = now;
                }
            }

            for (packet_bytes, target_addr, seq_num) in packets_to_send {
                // The send operation awaits, temporarily releasing the current task.
                match socket.send_to(&packet_bytes, target_addr).await {
                    Ok(_) => {
                        println!("[{}] Retransmitting Seq: {}", target_addr, seq_num);
                    }
                    Err(e) => {
                        eprintln!("[{}] Error retransmitting: {}", target_addr, e);
                    }
                }
            }
        }
    }

    /// --- Incoming Reliability: Receiving and Processing ---

    /// Processes a single received packet. This is separated from the receive loop.
    async fn process_received_packet(&self, buffer: &[u8], sender_addr: SocketAddr) {
        let packet = match Packet::from_bytes(buffer) {
            Some(p) => p,
            None => {
                println!("[{}] Ignoring malformed packet.", sender_addr);
                return;
            }
        };

        let mut peers = self.peers.lock().await;
        let state = peers
            .entry(sender_addr)
            .or_insert_with(|| PeerState::new(1));
        state.last_activity = Instant::now();

        // 1. Process Acknowledgements (ACKs) first
        // Retain only the packets whose sequence number is NOT acknowledged.
        let ack_num = packet.header.ack_num;
        let original_queue_len = state.unacked_queue.len();

        state
            .unacked_queue
            .retain(|unacked| unacked.seq_num >= ack_num);

        if state.unacked_queue.len() < original_queue_len {
            println!(
                "[{}] ACKed up to seq: {} ({} packets cleared)",
                sender_addr,
                ack_num - 1,
                original_queue_len - state.unacked_queue.len()
            );
        }

        // 2. Skip Data Sequence Check for Pure ACKs
        // If the packet has the ACK flag set AND no data payload, it is a pure ACK.
        // We skip the data sequence number check, as its seq_num is intentionally 0.
        if (packet.header.flags & FLAG_ACK) != 0 && packet.payload.is_empty() {
            // We have processed the ACK part (step 1) and can stop here.
            // Note: If a pure ACK is received, we do not need to send an ACK back.
            println!(
                "[{}] Received pure ACK (Seq {}). Data sequence check skipped.",
                sender_addr, packet.header.seq_num
            );
            return;
        }

        // 3. Process Incoming Data (Sequence Number & Ordering)
        let seq_num = packet.header.seq_num;
        let expected_seq = state.next_expected_seq_num;

        // Variable to hold the ACK number we need to send back
        let ack_to_send;

        if seq_num == expected_seq {
            // A. IN-ORDER: Correct packet received. Deliver payload.
            println!("[{}] Received IN-ORDER Seq: {}", sender_addr, seq_num);
            state.received_messages.push_back(packet.payload);
            state.next_expected_seq_num = state.next_expected_seq_num.wrapping_add(1);

            // B. Drain Reorder Buffer: Check if the next packets are waiting.
            self.drain_reorder_buffer(state);

            // ACK should acknowledge the next expected sequence number (window closure)
            ack_to_send = state.next_expected_seq_num;
        } else if seq_num > expected_seq {
            // C. OUT-OF-ORDER: Buffer the packet.
            println!(
                "[{}] Received OUT-OF-ORDER Seq: {}. Expected: {}",
                sender_addr, seq_num, expected_seq
            );
            state.reorder_buffer.insert(seq_num, packet.payload);

            // D. Send a *duplicate* ACK for the expected_seq (Fast Retransmit hint)
            ack_to_send = expected_seq;
        } else {
            // E. DUPLICATE: We already processed this data packet.
            println!(
                "[{}] Received DUPLICATE Seq: {}. Re-ACKing...",
                sender_addr, seq_num
            );
            // Re-ACK with the current expected sequence number.
            ack_to_send = expected_seq;
        }

        // Send ACK after processing data and dropping the lock.
        // This is crucial to stop the sender's retransmission timer immediately.
        // Must drop the lock before calling an async function that might acquire it.
        std::mem::drop(peers);
        self.send_ack_only(sender_addr, ack_to_send).await;
    }

    /// Sends an ACK packet back to the sender. This is now an async function.
    async fn send_ack_only(&self, target_addr: SocketAddr, ack_num: u32) {
        let ack_packet = Packet {
            header: PacketHeader {
                seq_num: 0,      // ACK packets don't need a sequence number on their own channel
                ack_num,         // The actual ACK value
                flags: FLAG_ACK, // Use the defined ACK flag
            },
            payload: Vec::new(),
        };
        // We ignore the result of the send operation here, as RUDP doesn't guarantee ACKs of ACKs.
        let _ = self
            .socket
            .send_to(&ack_packet.to_bytes(), target_addr)
            .await;
    }

    /// Checks the reorder buffer to see if packets expected next are now available.
    fn drain_reorder_buffer(&self, state: &mut PeerState) {
        loop {
            let next_seq = state.next_expected_seq_num;
            if let Some(payload) = state.reorder_buffer.remove(&next_seq) {
                // Found the next sequential packet! Deliver it and move the window.
                println!("[Drain] Delivered buffered Seq: {}", next_seq);
                state.received_messages.push_back(payload);
                state.next_expected_seq_num = state.next_expected_seq_num.wrapping_add(1);
            } else {
                // Gap found or buffer is empty. Stop draining.
                break;
            }
        }
    }

    /// Polls for delivered messages ready for the application. This is still sync as it just takes a lock.
    pub async fn poll_ready_message(&self, addr: &SocketAddr) -> Option<Vec<u8>> {
        let mut peers = self.peers.lock().await;

        if let Some(state) = peers.get_mut(addr) {
            return state.received_messages.pop_front();
        }
        None
    }

    /// The main continuous loop that processes incoming packets.
    pub async fn run_receiver_loop(self: Arc<Self>) {
        println!(
            "RUDP Wrapper receiver listening on {}",
            self.socket.local_addr().unwrap()
        );
        let mut buf = [0u8; MAX_PAYLOAD_SIZE + HEADER_SIZE];

        loop {
            // recv_from is an async operation that yields until a packet arrives.
            let (len, sender_addr) = match self.socket.recv_from(&mut buf).await {
                Ok(result) => result,
                Err(e) => {
                    eprintln!("Error receiving from socket: {}", e);
                    // Short sleep to prevent a tight loop on persistent errors
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            };

            // Process the received packet asynchronously
            self.process_received_packet(&buf[..len], sender_addr).await;
        }
    }

    /// The main continuous loop that handles retransmissions.
    pub async fn run_retransmitter_loop(self: Arc<Self>) {
        loop {
            // Wait for RTO duration before checking for timeouts
            tokio::time::sleep(RTO).await;
            self.handle_retransmissions().await;
        }
    }
}
