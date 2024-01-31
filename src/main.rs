use clap::{ arg, Command, Arg, ArgAction };
use std::{ fs, str::FromStr};
use std::net::{ SocketAddr, TcpStream };
use bittorrent_starter_rust::{ Torrent, bencode, tracker, network };
use hex;

fn main() {
    let matches = Command::new("bittorrent-client")
        .about("A BitTorrent client")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .subcommand(
            Command::new("decode")
                .about("Decodes a bencoded value")
                .arg(arg!(<ENCODED_VALUE> "The value to decode")),
        )
        .subcommand(
            Command::new("info")
                .about("Displays information about a torrent")
                .arg(arg!(<TORRENT_FILE> "The torrent file")),
        )
        .subcommand(
            Command::new("peers")
                .about("Lists peers for a torrent")
                .arg(arg!(<TORRENT_FILE> "The torrent file")),
        )
        .subcommand(
            Command::new("handshake")
                .about("Performs a handshake with a peer")
                .arg(arg!(<TORRENT_FILE> "The torrent file"))
                .arg(arg!(<PEER> "The peer to connect to")),
        )
        .subcommand(
            Command::new("download_piece")
                .about("Downloads a specific piece of a torrent")
                .arg(Arg::new("OUTPUT")
                    .short('o')
                    .long("output")
                    .action(ArgAction::Set)
                    .value_name("OUTPUT")
                    .help("Output file")
                    .required(true))
                .arg(arg!(<TORRENT_FILE> "The torrent file"))
                .arg(arg!(<PIECE_INDEX> "The index of the piece to download")),
        )
        .subcommand(
            Command::new("download")
                .about("Downloads the entire torrent")
                // .arg(arg!(-o --output <OUTPUT> "Output file").required(true))
                .arg(Arg::new("OUTPUT")
                    .short('o')
                    .long("output")
                    .action(ArgAction::Set)
                    .value_name("OUTPUT")
                    .help("Output file")
                    .required(true))
                .arg(arg!(<TORRENT_FILE> "The torrent file")),
        )
        .get_matches();

    match matches.subcommand() {
        Some(("decode", sub_matches)) => {
            let encoded_value = sub_matches.get_one::<String>("ENCODED_VALUE").unwrap();
            let (decoded_value, _encoded_tail) = bencode::decode_bencoded_value(encoded_value.as_bytes());
            println!("{}", decoded_value.to_string());
        },
        Some(("info", sub_matches)) => {
            let torrent_filename = sub_matches.get_one::<String>("TORRENT_FILE").unwrap();
            let content = fs::read(torrent_filename).unwrap();
            let torrent = Torrent::from_bytes(&content);
            display_torrent_info(&torrent);
        },
        Some(("peers", sub_matches)) => {
            let torrent_filename = sub_matches.get_one::<String>("TORRENT_FILE").unwrap();
            let content = fs::read(torrent_filename).unwrap();
            let torrent = Torrent::from_bytes(&content);
            let tracker_response = tracker::get_tracker(&torrent);
            for peer in tracker_response.peers {
                println!("{}", peer);
            }
        },
        Some(("handshake", sub_matches)) => {
            let torrent_filename = sub_matches.get_one::<String>("TORRENT_FILE").unwrap();
            let peer_address = sub_matches.get_one::<String>("PEER").unwrap();
            perform_handshake(torrent_filename, peer_address);
        },
        Some(("download_piece", sub_matches)) => {
            let output_filename = sub_matches.get_one::<String>("OUTPUT").unwrap();
            let torrent_filename = sub_matches.get_one::<String>("TORRENT_FILE").unwrap();
            let piece_index = sub_matches.get_one::<String>("PIECE_INDEX").unwrap().parse::<u32>().unwrap();
            download_single_piece(torrent_filename, output_filename, piece_index);
        },
        Some(("download", sub_matches)) => {
            dbg!(&sub_matches);
            let output_filename = sub_matches.get_one::<String>("OUTPUT").unwrap();
            let torrent_filename = sub_matches.get_one::<String>("TORRENT_FILE").unwrap();
            download_torrent(torrent_filename, output_filename);
        },
        _ => unreachable!(),
    }
}

fn display_torrent_info(torrent: &Torrent) {
    println!("Tracker URL: {}", torrent.announce);
    println!("Length: {}", torrent.info.length);
    let info_hash = torrent.get_info_hash();
    println!("Info Hash: {}", hex::encode(&info_hash));
    println!("Piece Length: {}", torrent.info.piece_length);
    println!("Piece Hashes:");
    for piece in torrent.info.pieces.chunks(20) {
        println!("{}", hex::encode(piece));
    }
}

fn perform_handshake(torrent_filename: &str, peer_address: &str) {
    let content = fs::read(torrent_filename).unwrap();
    let torrent: Torrent = serde_bencode::from_bytes(&content).unwrap();
    let peer = SocketAddr::from_str(peer_address).unwrap();
    let stream = TcpStream::connect(&peer).unwrap();
    let response_peer_id = network::perform_peer_handshake(&torrent, &stream).unwrap();
    println!("Peer ID: {}", hex::encode(&response_peer_id));
}

fn download_single_piece(torrent_filename: &str, output_filename: &str, piece_index: u32) {
    let content = fs::read(torrent_filename).unwrap();
    let torrent = Torrent::from_bytes(&content);
    let downloaded_piece: Vec<u8> = network::download_piece(piece_index, &torrent);
    assert!(torrent.is_piece_hash_correct(&downloaded_piece, piece_index));
    fs::write(output_filename, downloaded_piece).unwrap();
    println!("Piece {} downloaded to {}.", piece_index, output_filename);
}

fn download_torrent(torrent_filename: &str, output_filename: &str) {
    let content = fs::read(torrent_filename).unwrap();
    let torrent = Torrent::from_bytes(&content);
    let downloaded_pieces: Vec<u8> = network::download_and_verify_pieces(&torrent);
    fs::write(output_filename, &downloaded_pieces).unwrap();
    println!("Downloaded {} to {}", torrent_filename, output_filename);
}
