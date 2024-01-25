use serde_json;
use serde_bencode;
use std::{ env, fs, str, net::Ipv4Addr };
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
    length: usize,
    name: String,
    #[serde(rename = "piece length")]
    piece_length: usize,
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
    peers: Vec<Peer>,
}

impl TrackerResponse {
    fn from_intermediate(intermediate: IntermediateTrackerResponse) -> Self {
        let peers: Vec<Peer> = intermediate.peers.chunks(6).map(|chunk| {
            let arr: [u8; 6] = chunk.try_into().expect("Chunk must have length 6");
            Peer::from_bytes(&arr)
        }).collect();
        TrackerResponse {
            interval: intermediate.interval,
            peers,
        }
    }
}

#[derive(Debug)]
struct Peer {
    ip: Ipv4Addr,
    port: u16,
}

impl Peer {
    fn from_bytes(raw: &[u8; 6]) -> Self {
        let port_bytes = &raw[4..6];
        let port_byte_array: [u8; 2] = port_bytes.try_into().expect("slice with incorrect length");
        let port = u16::from_be_bytes(port_byte_array);
        Peer {
            ip: Ipv4Addr::new(raw[0], raw[1], raw[2], raw[3]),
            port,
        }
    }
}


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

fn get_tracker(torrent: &Torrent) -> TrackerResponse {
    let info_bytes = serde_bencode::to_bytes(&torrent.info).unwrap();
    let mut hasher = Sha1::new();
    hasher.update(&info_bytes);
    let info_hash = hasher.finalize();
    dbg!(hash_bytes(&info_bytes));
    dbg!(hex::encode(&info_hash));
    let info_hash_encoded = urlencoding::encode_binary(&info_hash); // Custom encoding
    dbg!(&info_hash_encoded);
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
                println!("{}:{}", peer.ip, peer.port);
            }
        },
        _ => println!("unknown command: {}", args[1])
    }
}
