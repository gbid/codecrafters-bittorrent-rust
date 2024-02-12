use std::net::{ SocketAddr };
use std::io;
use std::collections::VecDeque;
use std::fmt::Write;
use tokio::net::{ TcpStream };

use tokio::io::{ AsyncReadExt, AsyncWriteExt };
use crate::{ tracker, Torrent, BLOCK_SIZE };
use crate::peer_messages::{ PeerMessage, RequestPayload, PiecePayload };

use tokio::task;
use futures::stream::{ FuturesUnordered, StreamExt };
use std::sync::{ Arc };
use tokio::sync::{ Mutex };

pub async fn download_piece(piece_index: u32, torrent: Arc<Torrent>, peer: &SocketAddr) -> io::Result<Vec<u8>> {
    let mut stream = TcpStream::connect(peer).await?;
    perform_peer_handshake(torrent.clone(), &mut stream).await?;
    let msg = PeerMessage::from_reader(&mut stream).await?;
    if !matches!(msg, PeerMessage::Bitfield(_payload)) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Expected Bitfield reponse"));
    }
    PeerMessage::Interested.write_to(&mut stream).await?;
    let msg = PeerMessage::from_reader(&mut stream).await?;
    if !matches!(msg, PeerMessage::Unchoke) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Expected Unchoke reponse"));
    }
    // request blocks of piece
    let number_of_blocks = torrent.number_of_blocks(piece_index);
    let mut blocks: Vec<Option<Vec<u8>>> = vec![None; number_of_blocks.try_into().unwrap()];
    const MAX_REQUESTS: u32 = 10;
    let max_requests = u32::min(MAX_REQUESTS, number_of_blocks);
    let mut active_requests: VecDeque<u32> = VecDeque::with_capacity(MAX_REQUESTS.try_into().unwrap());
    for block_index in 0..max_requests {
        send_request(block_index, piece_index, torrent.clone(), &mut stream, &mut active_requests).await?;
    }
    for block_index in max_requests..number_of_blocks {
        if active_requests.len() < max_requests.try_into().unwrap() {
            send_request(block_index, piece_index, torrent.clone(), &mut stream, &mut active_requests).await?;
        }
        else {
            handle_response(&mut stream, &mut active_requests, &mut blocks).await?;
            send_request(block_index, piece_index, torrent.clone(), &mut stream, &mut active_requests).await?;
        }
    }
    while !active_requests.is_empty() {
        handle_response(&mut stream, &mut active_requests, &mut blocks).await?;
    }
    let piece: Vec<u8> = blocks
        .into_iter()
        .flat_map(|block| block.expect("All blocks must be successfully downloaded"))
        .collect();
    if torrent.is_piece_hash_correct(&piece, piece_index) {
        Ok(piece)
    }
    else {
        Err(io::Error::new(io::ErrorKind::InvalidData, "Piece hash mismatch"))
    }
}

async fn send_request(
    block_index: u32, piece_index: u32, torrent: Arc<Torrent>,
    stream: &mut TcpStream, active_requests: &mut VecDeque<u32>)
    -> io::Result<()> {
    let request_payload = RequestPayload {
        index: piece_index,
        begin: block_index*(u32::try_from(BLOCK_SIZE).unwrap()),
        length: torrent.block_size(block_index, piece_index)
    };
    PeerMessage::Request(request_payload).write_to(stream).await?;
    active_requests.push_back(block_index);
    Ok(())
}

async fn handle_response(stream: &mut TcpStream, active_requests: &mut VecDeque<u32>, piece: &mut [Option<Vec<u8>>]) -> io::Result<()> {
    let response_msg = PeerMessage::from_reader(stream).await?;
    match response_msg {
        PeerMessage::Piece(PiecePayload {
            index: _,
            begin,
            block,
        }) => {
            let block_index = begin / (u32::try_from(BLOCK_SIZE).unwrap());
            piece[usize::try_from(block_index).unwrap()] = Some(block);
            active_requests.retain(|&x| x != block_index);
            Ok(())
        },
        _ => {
            Err(io::Error::new(io::ErrorKind::InvalidData, format!("Expected PeerMessage::Piece with payload, got {:?}", response_msg)))
        },
    }
}


#[derive(Debug)]
pub enum DownloadError {
    PeerError { peer: SocketAddr, piece_index: u32, error: std::io::Error },
}

use std::fmt;
impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            DownloadError::PeerError { ref peer, piece_index, ref error } => {
                write!(f, "Error with peer {:?} on piece {}: {}", peer, piece_index, error)
            },
        }
    }
}

impl std::error::Error for DownloadError {}

struct DownloadSuccess {
    peer: SocketAddr,
    piece_index: u32,
    piece_data: Vec<u8>,
}

async fn try_download_piece(
    piece_index: u32,
    torrent: Arc<Torrent>,
    peer_queue: Arc<Mutex<VecDeque<SocketAddr>>>,
) -> Result<DownloadSuccess, DownloadError> {
    loop {
        let peer_option = {
            let mut queue = peer_queue.lock().await;
            queue.pop_front()
        };
        if let Some(peer) = peer_option {
            match download_piece(piece_index, torrent, &peer).await {
                Ok(piece_data) => return Ok(DownloadSuccess {
                    peer,
                    piece_index,
                    piece_data,
                }),
                Err(error) => return Err(DownloadError::PeerError {
                    peer,
                    piece_index,
                    error,
                }),
            }
        }
    }
}
pub async fn download_pieces(torrent: Arc<Torrent>) -> io::Result<Vec<u8>> {
    let number_of_pieces = (torrent.info.length + torrent.info.piece_length - 1) / torrent.info.piece_length;
    let peers = tracker::get_tracker(torrent.clone()).peers;
    let peer_queue = Arc::new(Mutex::new(VecDeque::from(peers)));
    let mut futures = FuturesUnordered::new();

    for piece_index in 0..number_of_pieces {
        let torrent_clone = torrent.clone();
        let peer_queue_clone = peer_queue.clone();
        let future = task::spawn(try_download_piece(piece_index, torrent_clone, peer_queue_clone));
        futures.push(future);
    }

    let mut pieces = vec![None; number_of_pieces.try_into().unwrap()];
    while let Some(result) = futures.next().await {
        match result {
            Ok(Ok(DownloadSuccess {
                peer,
                piece_index,
                piece_data,
            })) => {
                peer_queue.lock().await.push_front(peer);
                pieces[piece_index as usize] = Some(piece_data);
            },
            Ok(Err(DownloadError::PeerError {
                peer,
                piece_index,
                error: _,
            })) => {
                let torrent_clone = torrent.clone();
                peer_queue.lock().await.push_back(peer);
                let peer_queue_clone = peer_queue.clone();
                let future = task::spawn(try_download_piece(piece_index, torrent_clone, peer_queue_clone));
                futures.push(future);
            }
            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e.to_string())), // Handle spawn errors
        }
    }

    // Assemble the downloaded pieces into the final file content
    Ok(pieces.into_iter().flat_map(|piece| piece.unwrap()).collect())
}

pub struct Handshake {
    pub protocol_string_length: u8,
    pub protocol_string: [u8; 19],
    pub reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}
impl fmt::Debug for Handshake {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handshake")
            .field("protocol_string_length", &self.protocol_string_length)
            .field("protocol_string", &String::from_utf8_lossy(&self.protocol_string))
            .field("reserved", &self.reserved.iter().fold(String::new(), |mut output, b| {
                let _ = write!(output, "{b:02X}");
                output
            }))
            .field("info_hash", &hex::encode(self.info_hash))
            .field("peer_id", &hex::encode(self.peer_id))
            .finish()
    }
}
impl Handshake {
    fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Handshake {
        let protocol_string_length: u8 = 19;
        let protocol_string: [u8; 19] = b"BitTorrent protocol".to_owned();
        let reserved: [u8; 8] = [0; 8];
        Handshake {
            protocol_string_length,
            protocol_string,
            reserved,
            info_hash,
            peer_id,
        }
    }
    async fn write_to<W: AsyncWriteExt + Unpin>(&self, writer: &mut W) -> io::Result<()> {
        let mut handshake_request: Vec<u8> = Vec::with_capacity(68);
        handshake_request.push(self.protocol_string_length);
        handshake_request.extend_from_slice(&self.protocol_string);
        handshake_request.extend_from_slice(&self.reserved);
        handshake_request.extend_from_slice(&self.info_hash);
        handshake_request.extend_from_slice(&self.peer_id);
        writer.write_all(&handshake_request).await
    }
    async fn read_from<R: AsyncReadExt + Unpin>(reader: &mut R) -> io::Result<Handshake> {
        let mut response_buf = [0; 68];
        let response_bytes_read = reader.read_exact(&mut response_buf).await?;
        if response_bytes_read != 68 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid handshake response length"));
        }
        let protocol_string_length: u8 = response_buf[0];
        let protocol_string: [u8; 19] = response_buf[1..20]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        let reserved: [u8; 8] = response_buf[20..28]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        let info_hash: [u8; 20] = response_buf[28..48]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        let peer_id: [u8; 20] = response_buf[48..68]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        let handshake = Handshake {
            protocol_string_length,
            protocol_string,
            reserved,
            info_hash,
            peer_id,
        };
        Ok(handshake)
    }
}
pub async fn perform_peer_handshake(torrent: Arc<Torrent>, stream: &mut TcpStream) -> io::Result<Handshake> {
    let info_hash: [u8; 20] = torrent.get_info_hash();
    let my_peer_id: [u8; 20] = b"00112233445566778899".to_owned();
    let handshake_request = Handshake::new(info_hash, my_peer_id);
    handshake_request.write_to(stream).await?;
    let handshake_response = Handshake::read_from(stream).await?;
    if handshake_response.info_hash != info_hash {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Info hash mismatch"));
    }
    Ok(handshake_response)
}
