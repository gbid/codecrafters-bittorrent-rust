use serde_json;
use std::env;

// Available if you need it!
// use serde_bencode

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> (serde_json::Value, &str) {
    // If encoded_value starts with a digit, it's a number
    //
    dbg!(&encoded_value);
    let (head, mut tail) = encoded_value.split_at(1);
    let head: char = head.chars().next().expect("split_at(1) did not panic so the left part has to be nonempty");
    let (val, new_tail) = match head {
        'i' => {
            // let content = &encoded_value[1..].chars().take_while(|&ch| ch != 'e').collect::<String>();
            let end_index: usize = tail.find('e').unwrap();
            let value = tail[..end_index].parse::<i64>().unwrap();
            (value.into(), &tail[end_index + 1 ..])
        },
        'l' => {
            let mut values = Vec::new();
            while !tail.starts_with('e') {
                let (val, new_tail) = decode_bencoded_value(tail);
                tail = new_tail;
                values.push(val);
            }
            (values.into(), &tail[1..])
        },
        'd' => {
            let mut values = serde_json::Map::new();
            while !tail.starts_with('e') {
                let (key, new_tail) = decode_bencoded_value(tail);
                let (val, new_tail) = decode_bencoded_value(new_tail);
                tail = new_tail;
                values.insert(key.to_string(), val);
            }
            (values.into(), &tail[1..])
        },
        '0' ..= '9' => {
            // Example: "5:hello" -> "hello"
            let (head, tail) = encoded_value.split_once(':').unwrap();
            let value_length = head.parse::<usize>().unwrap();
            let value = tail[.. value_length].to_string();
            (value.into(), &tail[value_length ..])
        },
        _ =>  panic!("Unhandled encoded value: {}", encoded_value)
    };
    dbg!(&val);
    (val, new_tail)
}

// Usage: your_bittorrent.sh decode "<encoded_value>"
fn main() {
    let args: Vec<String> = env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let (decoded_value, _encoded_tail) = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value.to_string());
    } else {
        println!("unknown command: {}", args[1])
    }
}
