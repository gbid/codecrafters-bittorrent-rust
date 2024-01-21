use serde_json;
use std::{ env, fs, str };

// Available if you need it!
// use serde_bencode

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
            let (metainfo, _encoded_tail) = decode_bencoded_value(content);
            println!("Tracker URL: {}", metainfo["announce"]);
            println!("Length: {}", metainfo["info"]["length"]);
        },
        _ => println!("unknown command: {}", args[1])
    }
}
