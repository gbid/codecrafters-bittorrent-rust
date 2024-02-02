use std::net::{ SocketAddr };
use std::io;
use std::collections::VecDeque;
use tokio::net::{ TcpStream };

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use crate::{ tracker, Torrent, BLOCK_SIZE };
use crate::peer_messages::{ PeerMessage, RequestPayload, PiecePayload };

use tokio::task;
use futures::stream::{FuturesUnordered, StreamExt};
use std::collections::HashSet;
use std::sync::{ Arc };
use tokio::sync::{ Mutex };
use rand::prelude::*;

#[derive(Debug)]
enum DownloadPieceState {
    Handshake,
    Bitfield,
    Interested,
    Unchoke,
    Request,
}

pub async fn download_piece(piece_index: u32, torrent: Arc<Torrent>, peer: &SocketAddr) -> io::Result<Vec<u8>> {
    let mut state = DownloadPieceState::Handshake;
    let mut stream = TcpStream::connect(peer).await?;
    loop {
        match state {
            DownloadPieceState::Handshake => {
                let _peer_id_hash = perform_peer_handshake(torrent.clone(), &mut stream).await?;
                state = DownloadPieceState::Bitfield;
            },
            DownloadPieceState::Bitfield => {
                let msg = PeerMessage::from_reader(&mut stream).await?;
                match msg {
                    PeerMessage::Bitfield(_payload) => {
                        state = DownloadPieceState::Interested;
                    },
                    _ => { panic!("Expected Bitfield"); },
                };
            },
            DownloadPieceState::Interested => {
                let raw_msg = PeerMessage::to_bytes(&PeerMessage::Interested).unwrap();
                stream.write(&raw_msg).await?;
                state = DownloadPieceState::Unchoke;
            },
            DownloadPieceState::Unchoke => {
                let msg = PeerMessage::from_reader(&mut stream).await?;
                match msg {
                    PeerMessage::Unchoke => {
                        state = DownloadPieceState::Request;
                    },
                    _ => {
                        panic!("Expected Bitfield");
                    },
                };
            },
            DownloadPieceState::Request => {
                let mut piece: Vec<Option<Vec<u8>>> = vec![None; torrent.info.length.try_into().unwrap()];
                let number_of_blocks = torrent.number_of_blocks(piece_index);
                const MAX_REQUESTS: u32 = 5;
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
                        handle_response(&mut stream, &mut active_requests, &mut piece).await?;
                    }
                }

                while !active_requests.is_empty() {
                    handle_response(&mut stream, &mut active_requests, &mut piece).await?;
                }
                let piece: Vec<u8> = piece.into_iter().filter(Option::is_some).flatten().flatten().collect();
                if torrent.is_piece_hash_correct(&piece, piece_index) {
                    return Ok(piece)
                }
                else {
                    return Err(io::Error::new(io::ErrorKind::InvalidData, "Info hash mismatch"))
                }
            },
        }
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
    let raw_request_msg = PeerMessage::to_bytes(&PeerMessage::Request(request_payload)).unwrap();
    stream.write(&raw_request_msg).await?;
    active_requests.push_back(block_index);
    Ok(())
}

async fn handle_response(stream: &mut TcpStream, active_requests: &mut VecDeque<u32>, piece: &mut Vec<Option<Vec<u8>>>) -> io::Result<()> {
    let response_msg = PeerMessage::from_reader(stream).await?;
    match response_msg {
        PeerMessage::Piece(PiecePayload {
            index,
            begin,
            block,
        }) => {
            // TODO: verify index, begin
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

// pub async fn download_pieces(torrent: Arc<Torrent>) -> io::Result<Vec<u8>> {
//     let number_of_pieces: u32 = (torrent.info.length + torrent.info.piece_length - 1) / torrent.info.piece_length;
//     let mut pieces: Vec<Option<Vec<u8>>> = vec![None; number_of_pieces.try_into().unwrap()];
//     let peers = tracker::get_tracker(&torrent).peers;
//     // TODO: make this truly async
//     for piece_index in 0..number_of_pieces.try_into().unwrap() {
//         let piece: Vec<u8> = download_piece(piece_index, torrent, &peer).await?;
//         pieces[usize::try_from(piece_index).unwrap()] = Some(piece);
//     }
//     Ok(pieces.into_iter().map(|piece| piece.unwrap()).flatten().collect())
// }

pub async fn download_pieces(torrent: Arc<Torrent>) -> io::Result<Vec<u8>> {
    let number_of_pieces = (torrent.info.length + torrent.info.piece_length - 1) / torrent.info.piece_length;
    let peers = tracker::get_tracker(torrent.clone()).peers;
    let peer_queue = Arc::new(Mutex::new(VecDeque::from(peers)));
    let mut futures = FuturesUnordered::new();

    for piece_index in 0..number_of_pieces {
        // does torrent really have to be an Arc<Torrent>?
        // As an alternative, can we also just clone the Torrent to not having to deal with Arc?
        // How would that affect performance?
        let torrent_clone = torrent.clone();
        let peer_queue_clone = peer_queue.clone();
        let future = task::spawn(async move {
            // let mut queue: VecDeque<SocketAddr> = *peer_queue_clone.lock().unwrap();
            'spinning: loop {
                if let Some(peer) = peer_queue_clone.lock().await.pop_front() {
                    match download_piece(piece_index, torrent_clone, &peer).await {
                        Ok(piece_data) => return Ok((peer, piece_index, piece_data)),
                        Err(e) => return Err((peer, piece_index, e)),
                    }
                }
            }
        });
        futures.push(future);
    }

    let mut pieces = vec![None; number_of_pieces.try_into().unwrap()];
    while let Some(result) = futures.next().await {
        match result {
            Ok(Ok((peer, piece_index, piece_data))) => {
                peer_queue.lock().await.push_front(peer);
                pieces[piece_index as usize] = Some(piece_data);
            },
            Ok(Err((peer, piece_index, _e))) => {
                let torrent_clone = torrent.clone();
                let peer_queue_clone = peer_queue.clone();
                let future = task::spawn(async move {
                    // let mut queue: VecDeque<SocketAddr> = *peer_queue_clone.lock().unwrap();
                    'spinning: loop {
                        if let Some(peer) = peer_queue_clone.lock().await.pop_front() {
                            match download_piece(piece_index, torrent_clone, &peer).await {
                                Ok(piece_data) => return Ok((peer, piece_index, piece_data)),
                                Err(e) => return Err((peer, piece_index, e)),
                            }
                        }
                    }
                });
                futures.push(future);
            }
            Err(e) => return Err(io::Error::new(io::ErrorKind::Other, e.to_string())), // Handle spawn errors
        }
    }

    // Assemble the downloaded pieces into the final file content
    
    // Ok(pieces.into_iter().flatten().collect())
    Ok(pieces.into_iter().map(|piece| piece.unwrap()).flatten().collect())
}

pub async fn perform_peer_handshake(torrent: Arc<Torrent>, stream: &mut TcpStream) -> io::Result<[u8; 20]> {
    // let mut stream = TcpStream::connect(peer).unwrap();
    let protocol_string_length: &[u8; 1] = &[19; 1];
    let protocol_string: &[u8; 19] = "BitTorrent protocol"
        .as_bytes()
        .try_into()
        .expect("Failed to convert a fixed-size byte array");
    let reserved: &[u8; 8] = &[0; 8];
    let info_hash: [u8; 20] = torrent.get_info_hash();
    let peer_id: &[u8; 20] = "00112233445566778899"
        .as_bytes()
        .try_into()
        .expect("Failed to convert a fixed-size byte array");
    let mut handshake_request: Vec<u8> = Vec::with_capacity(68);
    handshake_request.extend_from_slice(protocol_string_length);
    handshake_request.extend_from_slice(protocol_string);
    handshake_request.extend_from_slice(reserved);
    handshake_request.extend_from_slice(&info_hash);
    handshake_request.extend_from_slice(peer_id);
    stream.write(&handshake_request).await?;
    let mut handshake_response = [0; 68];
    let response_bytes_read = stream.read(&mut handshake_response).await?;
    if response_bytes_read != 68 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid handshake response length"));
    }
    let response_info_hash: [u8; 20] = handshake_response[28..48]
        .try_into()
        .expect("Failed to convert a fixed-size byte array");
    if &response_info_hash != &info_hash {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Info hash mismatch"));
    }
    let response_peer_id: [u8; 20] = handshake_response[48..68]
        .try_into()
        .expect("Failed to convert a fixed-size byte array");
    Ok(response_peer_id)
}
