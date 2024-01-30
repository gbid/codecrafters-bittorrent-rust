fn perform_peer_handshake(torrent: &Torrent, mut stream: &TcpStream) -> [u8; 20] {
    // let mut stream = TcpStream::connect(peer).unwrap();
    let protocol_string_length: &[u8; 1] = &[19; 1];
    let protocol_string: &[u8; 19] = "BitTorrent protocol"
        .as_bytes()
        .try_into()
        .expect("Failed to convert a fixed-size byte array");
    let reserved: &[u8; 8] = &[0; 8];
    let infohash: &[u8; 20] = &get_info_hash(&torrent);
    let peer_id: &[u8; 20] = "00112233445566778899"
        .as_bytes()
        .try_into()
        .expect("Failed to convert a fixed-size byte array");
    let mut handshake_request: Vec<u8> = Vec::with_capacity(68);
    handshake_request.extend_from_slice(protocol_string_length);
    handshake_request.extend_from_slice(protocol_string);
    handshake_request.extend_from_slice(reserved);
    handshake_request.extend_from_slice(infohash);
    handshake_request.extend_from_slice(peer_id);
    stream.write(&handshake_request).unwrap();
    let mut handshake_response = [0; 68];
    let result = stream.read(&mut handshake_response);
    if let Ok(68) = result {
        let response_peer_id: [u8; 20] = handshake_response[48..68]
            .try_into()
            .expect("Failed to convert a fixed-size byte array");
        response_peer_id
    } else {
        panic!("Handshake not answered, got: {:?}", result)
    }
}

