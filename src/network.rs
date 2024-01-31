use std::net::TcpStream;
use std::io::{ Write, Read };
use crate::{ tracker, Torrent, BLOCK_SIZE };
use crate::peer_messages::{ PeerMessage, RequestPayload, PiecePayload };

#[derive(Debug)]
enum DownloadPieceState {
    Handshake,
    Bitfield,
    Interested,
    Unchoke,
    Request,
}

pub fn download_piece(piece_index: u32, torrent: &Torrent) -> Vec<u8> {
    //dbg!(torrent);
    let mut state = DownloadPieceState::Handshake;
    let peer = tracker::get_tracker(torrent).peers[0];
    //dbg!(&peer);
    let mut stream = TcpStream::connect(&peer).unwrap();
    loop {
        //dbg!(&state);
        match state {
            DownloadPieceState::Handshake => {
                let peer_id_hash = perform_peer_handshake(torrent, &stream).unwrap();
                assert_eq!(peer_id_hash, torrent.get_info_hash());
                //dbg!(hex::encode(peer_id_hash));
                // TODO: validate peer id: [u8; 20]
                state = DownloadPieceState::Bitfield;
            },
            DownloadPieceState::Bitfield => {
                let msg = PeerMessage::from_reader(&stream).unwrap();
                //dbg!(&msg);
                match msg {
                    PeerMessage::Bitfield(_payload) => {
                        state = DownloadPieceState::Interested;
                    },
                    _ => { panic!("Expected Bitfield"); },
                };
            },
            DownloadPieceState::Interested => {
                let raw_msg = PeerMessage::to_bytes(&PeerMessage::Interested).unwrap();
                //dbg!(&raw_msg);
                stream.write(&raw_msg).unwrap();
                state = DownloadPieceState::Unchoke;
            },
            DownloadPieceState::Unchoke => {
                let msg = PeerMessage::from_reader(&stream).unwrap();
                //dbg!(&msg);
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
                let mut piece: Vec<u8> = Vec::with_capacity(torrent.info.length.try_into().unwrap());
                for block_index in 0..torrent.number_of_blocks(piece_index) {
                    //dbg!(block_index);
                    let request_payload = RequestPayload {
                        index: piece_index,
                        begin: block_index*(u32::try_from(BLOCK_SIZE).unwrap()),
                        length: torrent.block_size(block_index, piece_index)
                    };
                    //dbg!(&request_payload);
                    let raw_request_msg = PeerMessage::to_bytes(&PeerMessage::Request(request_payload)).unwrap();
                    //dbg!(&raw_request_msg);
                    stream.write(&raw_request_msg).unwrap();
                    let response_msg = PeerMessage::from_reader(&stream).unwrap();
                    //let mut dbg_buf = vec![0; 1];
                    //stream.read_exact(&mut dbg_buf).unwrap();
                    //dbg!(&response_msg);
                    match response_msg {
                        PeerMessage::Piece(PiecePayload {
                            index: _,
                            begin: _,
                            block,
                        }) => {
                            // TODO: verify index, begin
                            piece.extend_from_slice(&block);
                        },
                        _ => {
                            panic!("Expected PeerMessage::Piece, got: {:?}", response_msg);
                        },
                    };
                };
                // TODO: validate piece based in sha1 hash of torrent.info.pieces
                return piece;
            },
        }
    }
}

// pub fn download_piece_from_peer(piece_index: u32, torrent: &Torrent, peer: &SocketAddr) -> Option<Vec<u8>> {
//     return None;
//     //dbg!(torrent);
//     let mut state = DownloadPieceState::Handshake;
//     let peer = tracker::get_tracker(torrent).peers[0];
//     //dbg!(&peer);
//     let mut stream = TcpStream::connect(&peer).unwrap();
//     loop {
//         //dbg!(&state);
//         match state {
//             DownloadPieceState::Handshake => {
//                 let _peer_id_hash = perform_peer_handshake(torrent, &stream)?;
//                 //dbg!(hex::encode(peer_id_hash));
//                 // TODO: validate peer id: [u8; 20]
//                 state = DownloadPieceState::Bitfield;
//             },
//             DownloadPieceState::Bitfield => {
//                 let msg = PeerMessage::from_reader(&stream).unwrap();
//                 //dbg!(&msg);
//                 match msg {
//                     PeerMessage::Bitfield(_payload) => {
//                         state = DownloadPieceState::Interested;
//                     },
//                     _ => { panic!("Expected Bitfield"); },
//                 };
//             },
//             DownloadPieceState::Interested => {
//                 let raw_msg = PeerMessage::to_bytes(&PeerMessage::Interested).unwrap();
//                 //dbg!(&raw_msg);
//                 stream.write(&raw_msg).unwrap();
//                 state = DownloadPieceState::Unchoke;
//             },
//             DownloadPieceState::Unchoke => {
//                 let msg = PeerMessage::from_reader(&stream).unwrap();
//                 //dbg!(&msg);
//                 match msg {
//                     PeerMessage::Unchoke => {
//                         state = DownloadPieceState::Request;
//                     },
//                     _ => {
//                         panic!("Expected Bitfield");
//                     },
//                 };
//             },
//             DownloadPieceState::Request => {
//                 let mut piece: Vec<u8> = Vec::with_capacity(torrent.info.length.try_into().unwrap());
//                 for block_index in 0..torrent.number_of_blocks(piece_index) {
//                     //dbg!(block_index);
//                     let request_payload = RequestPayload {
//                         index: piece_index,
//                         begin: block_index*(u32::try_from(BLOCK_SIZE).unwrap()),
//                         length: torrent.block_size(block_index, piece_index)
//                     };
//                     //dbg!(&request_payload);
//                     let raw_request_msg = PeerMessage::to_bytes(&PeerMessage::Request(request_payload)).unwrap();
//                     //dbg!(&raw_request_msg);
//                     stream.write(&raw_request_msg).unwrap();
//                     let response_msg = PeerMessage::from_reader(&stream).unwrap();
//                     //let mut dbg_buf = vec![0; 1];
//                     //stream.read_exact(&mut dbg_buf).unwrap();
//                     //dbg!(&response_msg);
//                     match response_msg {
//                         PeerMessage::Piece(PiecePayload {
//                             index: _,
//                             begin: _,
//                             block,
//                         }) => {
//                             // TODO: verify index, begin
//                             piece.extend_from_slice(&block);
//                         },
//                         _ => {
//                             panic!("Expected PeerMessage::Piece, got: {:?}", response_msg);
//                         },
//                     };
//                 };
//                 // TODO: validate piece based in sha1 hash of torrent.info.pieces
//                 return piece;
//             },
//         }
//     }
// }

pub fn download_and_verify_pieces(torrent: &Torrent) -> Vec<u8> {
    let number_of_pieces: u32 = (torrent.info.length + torrent.info.piece_length - 1) / torrent.info.piece_length;
    let mut pieces: Vec<Option<Vec<u8>>> = vec![None; number_of_pieces.try_into().unwrap()];
    for piece_index in 0..number_of_pieces.try_into().unwrap() {
        let piece = download_piece(piece_index, torrent);
        assert!(torrent.is_piece_hash_correct(&piece, piece_index));
        pieces[usize::try_from(piece_index).unwrap()] = Some(piece);
    }
    pieces.into_iter().map(|piece| piece.unwrap()).flatten().collect()
}

pub fn perform_peer_handshake(torrent: &Torrent, mut stream: &TcpStream) -> Option<[u8; 20]> {
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
    stream.write(&handshake_request).unwrap();
    let mut handshake_response = [0; 68];
    let result = stream.read(&mut handshake_response);
    if let Ok(68) = result {
        let response_info_hash: [u8; 20] = handshake_response[28..48]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        if &response_info_hash != &info_hash {
            // TODO: send Cancel PeerMessage
            return None;
        }
        let response_peer_id: [u8; 20] = handshake_response[48..68]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        Some(response_peer_id)
    } else {
        None
    }
}
