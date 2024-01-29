use serde_json;
use serde_bencode;
use std::{ env, fs, str};
use std::net::{ Ipv4Addr, TcpStream, SocketAddr };
use std::str::FromStr;
use std::io;
use std::io::{ Write, Read };
use sha1::{ Sha1, Digest };
use serde::{ Serialize, Deserialize };
use serde_with::{ Bytes, serde_as };
use hex;
use urlencoding;

// Available if you need it!
// use serde_bencode


#[derive(Serialize, Deserialize, Debug)]
struct Torrent {
    announce: String,
    info: Info,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
struct Info {
    length: u32,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: u32,
    #[serde_as(as = "Bytes")]
    pieces: Vec<u8>,
}

#[serde_as]
#[derive(Deserialize, Debug)]
struct IntermediateTrackerResponse {
    interval: i64,
    //#[serde(rename = "min interval")]
    //min_interval: i64,
    #[serde_as(as = "Bytes")]
    peers: Vec<u8>,
    //complete: i64,
    //incomplete: i64,
}

#[derive(Debug)]
struct TrackerResponse {
    interval: i64,
    peers: Vec<SocketAddr>,
}

impl TrackerResponse {
    fn from_intermediate(intermediate: IntermediateTrackerResponse) -> Self {
        let peers: Vec<SocketAddr> = intermediate.peers.chunks(6).map(|chunk| {
            let arr: [u8; 6] = chunk.try_into().expect("Chunk must have length 6");
            SocketAddr::from_bytes(&arr)
        }).collect();
        TrackerResponse {
            interval: intermediate.interval,
            peers,
        }
    }
}
// 16kiB
const BLOCK_SIZE: usize = 16*1024;
trait SocketAddrExt {
    fn from_bytes(raw: &[u8; 6]) -> Self;
}
impl SocketAddrExt for SocketAddr {
    fn from_bytes(raw: &[u8; 6]) -> SocketAddr {
        let ip = Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3]);
        let port_bytes = &raw[4..6];
        let port_byte_array: [u8; 2] = port_bytes.try_into().expect("slice with incorrect length");
        let port = u16::from_be_bytes(port_byte_array);
        SocketAddr::new(ip.into(), port)
    }
}

#[derive(Debug)]
enum PeerMessage {
    Bitfield(Vec<u8>),
    Interested,
    Unchoke,
    Request(RequestPayload),
    Piece(PiecePayload),
}

impl PeerMessage {
    const ID_BITFIELD: u8 = 5;
    const ID_INTERESTED: u8 = 2;
    const ID_UNCHOKE: u8 = 1;
    const ID_REQUEST: u8 = 6;
    const ID_PIECE: u8 = 7;
    fn read_from_tcp_stream(mut stream: &TcpStream) -> io::Result<PeerMessage> {
        // let mut stream = TcpStream::connect(peer)?;
        // read length prefix (4 bytes)
        let mut length_buf = [0u8; 4];
        stream.read_exact(&mut length_buf).unwrap();
        let length = u32::from_be_bytes(length_buf);
        // read message id (1 byte)
        let mut id_buf = [0u8; 1];
        stream.read_exact(&mut id_buf).unwrap();
        let id = u8::from_be_bytes(id_buf);
        // read payload (of length as indicated in prefix bytes)
        dbg!(length);
        dbg!(id);
        // let payload_length: usize = length.try_into().unwrap() - 1;
        let payload_length: usize = <u32 as TryInto<usize>>::try_into(length).unwrap() - 1;
        dbg!(payload_length);
        let mut payload_buf: Vec<u8> = vec![0; payload_length];
        stream.read_exact(&mut payload_buf).unwrap();
        dbg!(&payload_buf);
        let msg = match id {
            PeerMessage::ID_BITFIELD => Ok(PeerMessage::Bitfield(payload_buf)),
            PeerMessage::ID_INTERESTED => Ok(PeerMessage::Interested),
            PeerMessage::ID_UNCHOKE => Ok(PeerMessage::Unchoke),
            PeerMessage::ID_REQUEST => Ok(PeerMessage::Request(RequestPayload::from_bytes(&payload_buf)?)),
            PeerMessage::ID_PIECE => Ok(PeerMessage::Piece(PiecePayload::from_bytes(payload_buf)?)),
            _ => Err(io::Error::new(io::ErrorKind::InvalidData,
                    format!("Unkown message type id: length: {}, id: {}, payload: {:?}", length, id, payload_buf)
                    )),
        };
        msg
    }
    fn to_bytes(&self) -> io::Result<Vec<u8>> {
        let mut buffer = Vec::new();
        match self {
            PeerMessage::Bitfield(_payload_buf) => {
                todo!()
            },
            PeerMessage::Interested => {
                let length: u32 = 1;
                let id: u8 = PeerMessage::ID_INTERESTED;
                buffer.extend_from_slice(&length.to_be_bytes());
                buffer.push(id);
            },
            PeerMessage::Unchoke => {
                todo!()
            },
            PeerMessage::Request(request_payload) => {
                let length: u32 = 1 + 3*4;
                let id: u8 = PeerMessage::ID_REQUEST;
                buffer.extend_from_slice(&length.to_be_bytes());
                buffer.push(id);
                buffer.extend_from_slice(&request_payload.index.to_be_bytes());
                buffer.extend_from_slice(&request_payload.begin.to_be_bytes());
                buffer.extend_from_slice(&request_payload.length.to_be_bytes());
            },
            PeerMessage::Piece(_piece_payload) => {
                todo!()
            },
        }
        Ok(buffer)
    }
}

#[derive(Debug)]
struct RequestPayload {
    index: u32,
    begin: u32,
    length: u32,
}
impl RequestPayload {
    fn from_bytes(raw: &[u8]) -> io::Result<RequestPayload> {
        if raw.len() != 12 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Cannot parse payload as RequestPayload"));
        }
        let index = u32::from_be_bytes(raw[0..4].try_into().unwrap());
        let begin = u32::from_be_bytes(raw[4..8].try_into().unwrap());
        let length = u32::from_be_bytes(raw[8..12].try_into().unwrap());
        Ok(RequestPayload {
            index,
            begin,
            length,
        })
    }
}
#[derive(Debug)]
struct PiecePayload {
    index: u32,
    begin: u32,
    block: Vec<u8>,
}
impl PiecePayload {
    fn from_bytes(mut raw: Vec<u8>) -> io::Result<PiecePayload> {
        if raw.len() < 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Cannot parse payload as PiecePayload"));
        }
        let index = u32::from_be_bytes(raw[0..4].try_into().unwrap());
        let begin = u32::from_be_bytes(raw[4..8].try_into().unwrap());
        let block = raw.split_off(8);
        Ok(PiecePayload {
            index,
            begin,
            block,
        })
    }
}
#[derive(Debug)]
enum DownloadPieceState {
    Handshake,
    Bitfield,
    Interested,
    Unchoke,
    Request,
}

fn download_piece(torrent: &Torrent, piece_index: u32) -> Vec<u8> {
    dbg!(torrent);
    let mut state = DownloadPieceState::Handshake;
    let peer = get_tracker(torrent).peers[1];
    dbg!(&peer);
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
    dbg!(this_pieces_size);
    dbg!(number_of_blocks);
    loop {
        dbg!(&state);
        match state {
            DownloadPieceState::Handshake => {
                let peer_id_hash = perform_peer_handshake(torrent, &stream);
                dbg!(hex::encode(peer_id_hash));
                // TODO: validate peer id: [u8; 20]
                state = DownloadPieceState::Bitfield;
            },
            DownloadPieceState::Bitfield => {
                let msg = PeerMessage::read_from_tcp_stream(&stream).unwrap();
                dbg!(&msg);
                match msg {
                    PeerMessage::Bitfield(_payload) => {
                        state = DownloadPieceState::Interested;
                    },
                    _ => { panic!("Expected Bitfield"); },
                };
            },
            DownloadPieceState::Interested => {
                let raw_msg = PeerMessage::to_bytes(&PeerMessage::Interested).unwrap();
                dbg!(&raw_msg);
                stream.write(&raw_msg).unwrap();
                state = DownloadPieceState::Unchoke;
            },
            DownloadPieceState::Unchoke => {
                let msg = PeerMessage::read_from_tcp_stream(&stream).unwrap();
                dbg!(&msg);
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
                dbg!(block_size);
                let mut piece: Vec<u8> = Vec::with_capacity(torrent.info.length.try_into().unwrap());
                for i in 0..number_of_blocks {
                    dbg!(i);
                    let this_blocks_size: u32 = if i == number_of_blocks - 1 {
                        (this_pieces_size % block_size).try_into().unwrap()
                    } else {
                        u32::try_from(block_size).unwrap()
                    };
                    dbg!(this_blocks_size);
                    let request_payload = RequestPayload {
                        index: piece_index,
                        begin: i*block_size,
                        length: this_blocks_size,
                    };
                    dbg!(&request_payload);
                    let raw_request_msg = PeerMessage::to_bytes(&PeerMessage::Request(request_payload)).unwrap();
                    dbg!(&raw_request_msg);
                    stream.write(&raw_request_msg).unwrap();
                    let response_msg = PeerMessage::read_from_tcp_stream(&stream).unwrap();
                    //let mut dbg_buf = vec![0; 1];
                    //stream.read_exact(&mut dbg_buf).unwrap();
                    dbg!(&response_msg);
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

/*
 * Peer Messages
 * length prefix (4 bytes)
 * message id (1 byte)
 * payload (var size)
 *
 * Bitfield:
 *  id: 5
 *  payload: some
 *
 *
 * Interested:
 *  id: 2
 *  payload: empty
 *
 * Unchoke:
 *  id: 1
 *  payload: empty
 *
 * Request:
 *  id: 6
 *  payload:
 *      index: piece index (byte?)
 *      begin: byte offset within piece (n * 2^14)
 *      length: length of block (2^14 or last block)
 * Piece:
 *   id: 7
 *   payload:
 *     index: piece index (byte?)
 *     begin: byte offset within piece (n * 2^14)
 *     block: data for the piece (<= 2^14 bytes?)
 *
 *
 *  Process for each Piece:
 *       Handshake
 *      Wait: Bitfield
 *      Send: Interested
 *      Wait: Unchoke
 *      Partition piece in to block of size 16kiB (based on metainfo)
 *          For each block:
 *              Send: Request
 *              Wait: Response
 *      Combine blocks into Piece
 *      Check Integrity of Piece
 *      
*/

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &[u8]) -> (serde_json::Value, &[u8]) {
    // If encoded_value starts with a digit, it's a number
    //
    dbg!(&encoded_value);
    let (head, mut tail) = encoded_value.split_first().unwrap();
    let (val, new_tail) = match head {
        b'i' => {
            // let content = &encoded_value[1..].chars().take_while(|&ch| ch != b'e').collect::<String>();
            let end_index: usize = tail.iter().position(|&x| x == b'e').unwrap();
            let value = str::from_utf8(&tail[..end_index]).unwrap().parse::<i64>().unwrap();
            (value.into(), &tail[end_index + 1 ..])
        },
        b'l' => {
            let mut values = Vec::new();
            while !tail.starts_with(&[b'e']) {
                let (val, new_tail) = decode_bencoded_value(tail);
                tail = new_tail;
                values.push(val);
            }
            (values.into(), &tail[1..])
        },
        b'd' => {
            let mut values = serde_json::Map::new();
            while !tail.starts_with(&[b'e']) {
                let (key, new_tail) = decode_bencoded_value(tail);
                let (val, new_tail) = decode_bencoded_value(new_tail);
                tail = new_tail;
                values.insert(key.as_str().unwrap().to_string(), val);
            }
            (values.into(), &tail[1..])
        },
        b'0' ..= b'9' => {
            // Example: "5:hello" -> "hello"
            let colon_index = encoded_value.iter().position(|&x| x == b':').unwrap();
            let (head, tail) = encoded_value.split_at(colon_index);
            let tail = &tail[1..];
            let value_length = str::from_utf8(&head).unwrap().parse::<usize>().unwrap();
            let value = String::from_utf8_lossy(&tail[.. value_length]);
            (value.into(), &tail[value_length ..])
        },
        _ =>  panic!("Unhandled encoded value: {:?}", encoded_value)
    };
    dbg!(&val);
    (val, new_tail)
}

fn hash_bytes(piece: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(piece);
    hex::encode(hasher.finalize())
}

fn get_info_hash(torrent: &Torrent) -> [u8; 20] {
    let info_bytes = serde_bencode::to_bytes(&torrent.info).unwrap();
    let mut hasher = Sha1::new();
    hasher.update(&info_bytes);
    let info_hash = hasher.finalize();
    info_hash.into()
}

fn get_tracker(torrent: &Torrent) -> TrackerResponse {
    let info_hash = get_info_hash(torrent);
    let info_hash_encoded = urlencoding::encode_binary(&info_hash); // Custom encoding
    let peer_id = "00112233445566778899";
    let port = "6881";
    let uploaded = "0";
    let downloaded = "0";
    let left = torrent.info.length.to_string();
    let compact = "1";

    let query_string = format!(
        "?info_hash={}&peer_id={}&port={}&uploaded={}&downloaded={}&left={}&compact={}",
        info_hash_encoded, peer_id, port, uploaded, downloaded, left, compact
    );

    let full_url = format!("{}{}", torrent.announce, query_string);
    let response = reqwest::blocking::Client::new()
        .get(&full_url)
        .send()
        .unwrap();

    let response_body = response.bytes().unwrap();
    dbg!(&response_body);
    let intermediate_tracker_response: IntermediateTrackerResponse = serde_bencode::from_bytes(&response_body).unwrap();
    let tracker_response = TrackerResponse::from_intermediate(intermediate_tracker_response);
    dbg!(&tracker_response);
    tracker_response
}

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
// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];
    match command.as_str() {
        "decode" => {
            let encoded_value = &args[2];
            let (decoded_value, _encoded_tail) = decode_bencoded_value(encoded_value.as_bytes());
            println!("{}", decoded_value.to_string());
        },
        "info" => {
            let torrent_filename = &args[2];
            let content: &Vec<u8> = &fs::read(torrent_filename).unwrap();
            let torrent_result: Result<Torrent, _> = serde_bencode::from_bytes(content);
            if let Ok(torrent) = torrent_result {
                println!("Tracker URL: {}", torrent.announce);
                println!("Length: {}", torrent.info.length);
                let info_bytes = serde_bencode::to_bytes(&torrent.info).unwrap();
                println!("Info Hash: {}", hash_bytes(&info_bytes));
                println!("Piece Length: {}", torrent.info.piece_length);
                println!("Piece Hashes:");
                for piece in torrent.info.pieces.chunks(20) {
                    println!("{}", hex::encode(piece));
                }
            } else {
                dbg!(&torrent_result);
            }
        },
        "peers" => {
            let torrent_filename = &args[2];
            let content: &Vec<u8> = &fs::read(torrent_filename).unwrap();
            let torrent: Torrent = serde_bencode::from_bytes(content).unwrap();
            let tracker_response = get_tracker(&torrent);
            for peer in tracker_response.peers {
                println!("{}", peer);
            }
        },
        "handshake" => {
            let torrent_filename = &args[2];
            let content: &Vec<u8> = &fs::read(torrent_filename).unwrap();
            let torrent: Torrent = serde_bencode::from_bytes(content).unwrap();
            let peer = SocketAddr::from_str(&args[3]).unwrap();
            let stream = TcpStream::connect(&peer).unwrap();
            let response_peer_id = perform_peer_handshake(&torrent, &stream);
            println!("Peer ID: {}", hex::encode(&response_peer_id));
            //let (ip, port)  = peer.split_once(":").unwrap();
        },
        "download_piece" => {
            // TODO: properly parse "-o" cli option
            let output_filename = &args[3];
            let torrent_filename = &args[4];
            let content: &Vec<u8> = &fs::read(torrent_filename).unwrap();
            let torrent: Torrent = serde_bencode::from_bytes(content).unwrap();
            let piece_index = u32::from_str(&args[5]).unwrap();
            let downloaded_piece: Vec<u8> = download_piece(&torrent, piece_index);
            // TODO: write downloaded_piece to output_file
            fs::write(output_filename, downloaded_piece).unwrap();
            println!("Piece {} downloaded to {}.", piece_index, output_filename);
        }
        _ => println!("unknown command: {}", args[1])
    }
}
