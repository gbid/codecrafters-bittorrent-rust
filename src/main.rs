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
                let info_hash = get_info_hash(&torrent)
                println!("Info Hash: {}", hex::encode(&info_hash));
                println!("Piece Length: {}", torrent.info.piece_length);
                println!("Piece Hashes:");
                for piece in torrent.info.pieces.chunks(20) {
                    println!("{}", hex::encode(piece));
                }
            } else {
                //dbg!(&torrent_result);
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

            assert!(is_piece_hash_correct(&downloaded_piece, piece_index, &torrent));
            fs::write(output_filename, downloaded_piece).unwrap();
            println!("Piece {} downloaded to {}.", piece_index, output_filename);
        },
        "download" => {
            // your_bittorrent.sh download -o /tmp/test.txt sample.torrent
            // TODO: properly parse "-o" cli option
            let output_filename = &args[3];
            let torrent_filename = &args[4];
            let content: &Vec<u8> = &fs::read(torrent_filename).unwrap();
            let torrent: Torrent = serde_bencode::from_bytes(content).unwrap();
            let downloaded_pieces: Vec<u8> = download_and_verify_pieces(&torrent);
            fs::write(output_filename, &downloaded_pieces).unwrap();

            println!("Downloaded {} to {}", torrent_filename, output_filename);
            // and here’s the output it expects:
            // Downloaded test.torrent to /tmp/test.txt.
        },
        _ => println!("unknown command: {}", args[1])
    }
}
