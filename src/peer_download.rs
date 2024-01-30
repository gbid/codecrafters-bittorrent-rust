#[derive(Debug)]
enum DownloadPieceState {
    Handshake,
    Bitfield,
    Interested,
    Unchoke,
    Request,
}

fn download_piece(torrent: &Torrent, piece_index: u32) -> Vec<u8> {
    //dbg!(torrent);
    let mut state = DownloadPieceState::Handshake;
    let peer = get_tracker(torrent).peers[1];
    //dbg!(&peer);
    let mut stream = TcpStream::connect(&peer).unwrap();
    // div_ceil not supported by Rust version of codecrafters.io:
    // let total_number_of_pieces: u32 = torrent.info.length.div_ceil(torrent.info.piece_length);
    let total_number_of_pieces: u32 = (torrent.info.length + torrent.info.piece_length - 1) / torrent.info.piece_length;
    let this_pieces_size = if piece_index == total_number_of_pieces - 1 {
        torrent.info.length % torrent.info.piece_length
    } else {
        torrent.info.piece_length
    };
    let block_size: u32 = BLOCK_SIZE.try_into().unwrap();
    // div_ceil not supported by Rust version of codecrafters.io:
    // let number_of_blocks = this_pieces_size.div_ceil(block_size);
    let number_of_blocks: u32 = (this_pieces_size + block_size - 1) / block_size;
    //dbg!(this_pieces_size);
    //dbg!(number_of_blocks);
    loop {
        //dbg!(&state);
        match state {
            DownloadPieceState::Handshake => {
                let _peer_id_hash = perform_peer_handshake(torrent, &stream);
                //dbg!(hex::encode(peer_id_hash));
                // TODO: validate peer id: [u8; 20]
                state = DownloadPieceState::Bitfield;
            },
            DownloadPieceState::Bitfield => {
                let msg = PeerMessage::read_from_tcp_stream(&stream).unwrap();
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
                let msg = PeerMessage::read_from_tcp_stream(&stream).unwrap();
                //dbg!(&msg);
                match msg {
                    PeerMessage::Unchoke => {
                        state = DownloadPieceState::Request;
                    },
                    _ => {
                        panic!("Expected Bitfield");
                    },
                };
                // TODO: validate pieces based in sha1 hash of torrent.info.pieces
            },
            DownloadPieceState::Request => {
                //dbg!(block_size);
                let mut piece: Vec<u8> = Vec::with_capacity(torrent.info.length.try_into().unwrap());
                for i in 0..number_of_blocks {
                    //dbg!(i);
                    let this_blocks_size: u32 = if i == number_of_blocks - 1 && this_pieces_size % block_size != 0 {
                        (this_pieces_size % block_size).try_into().unwrap()
                    } else {
                        u32::try_from(block_size).unwrap()
                    };
                    //dbg!(this_blocks_size);
                    let request_payload = RequestPayload {
                        index: piece_index,
                        begin: i*block_size,
                        length: this_blocks_size,
                    };
                    //dbg!(&request_payload);
                    let raw_request_msg = PeerMessage::to_bytes(&PeerMessage::Request(request_payload)).unwrap();
                    //dbg!(&raw_request_msg);
                    stream.write(&raw_request_msg).unwrap();
                    let response_msg = PeerMessage::read_from_tcp_stream(&stream).unwrap();
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
                return piece;
            },
        }
    }
}

fn download_and_verify_pieces(torrent: &Torrent) -> Vec<u8> {
    let number_of_pieces: u32 = (torrent.info.length + torrent.info.piece_length - 1) / torrent.info.piece_length;
    let mut pieces: Vec<Option<Vec<u8>>> = vec![None; number_of_pieces.try_into().unwrap()];
    for piece_index in 0..number_of_pieces.try_into().unwrap() {
        let piece = download_piece(torrent, piece_index);
        assert!(is_piece_hash_correct(&piece, piece_index, torrent));
        pieces[usize::try_from(piece_index).unwrap()] = Some(piece);
    }
    pieces.into_iter().map(|piece| piece.unwrap()).flatten().collect()
}

fn is_piece_hash_correct(piece: &[u8], piece_index: u32, torrent: &Torrent) -> bool {
    let a: usize = 20 * usize::try_from(piece_index).unwrap();
    let b: usize = 20 * usize::try_from(piece_index + 1).unwrap();
    let hash = |bytes: &[u8]| -> [u8; 20] {
        let mut hasher = Sha1::new();
        hasher.update(bytes);
        hasher.finalize().into()
    };
    torrent.info.pieces[a..b] == hash(&piece)
}

