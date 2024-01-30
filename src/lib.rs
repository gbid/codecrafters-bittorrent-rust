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

// 16kiB
const BLOCK_SIZE: usize = 16*1024;

fn get_info_hash(torrent: &Torrent) -> [u8; 20] {
    let info_bytes = serde_bencode::to_bytes(&torrent.info).unwrap();
    let mut hasher = Sha1::new();
    hasher.update(&info_bytes);
    let info_hash = hasher.finalize();
    info_hash.into()
}

