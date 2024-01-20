use serde_json;
use std::env;

// Available if you need it!
// use serde_bencode

#[allow(dead_code)]
fn decode_bencoded_value(encoded_value: &str) -> (serde_json::Value, &str) {
    // If encoded_value starts with a digit, it's a number
    //
    match encoded_value.chars().next().unwrap() {
        'i' => {
            // let content = &encoded_value[1..].chars().take_while(|&ch| ch != 'e').collect::<String>();
            let end_index: usize = encoded_value[1..].find('e').unwrap() + 1;
            let value = encoded_value[1..end_index].parse::<i64>().unwrap();
            dbg!(&encoded_value[end_index + 1 ..]);
            (value.into(), &encoded_value[end_index + 1 ..])
        },
        'l' => {
            let mut values = Vec::new();
            let mut tail = &encoded_value[1..];
            dbg!(&tail);
            while tail.chars().next().unwrap() != 'e' {
                let (val, tail2) = decode_bencoded_value(tail);
                dbg!(&val);
                dbg!(&tail2);
                tail = tail2;
                values.push(val);
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
    }
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
