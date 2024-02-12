use serde::{Deserialize, Serialize};
use serde_with::{serde_as, Bytes};
use sha1::{Digest, Sha1};

pub mod bencode;
pub mod network;
mod peer_messages;
pub mod tracker;

#[derive(Serialize, Deserialize, Debug)]
pub struct Torrent {
    pub announce: String,
    pub info: Info,
}

impl Torrent {
    pub fn from_bytes(bytes: &[u8]) -> Torrent {
        serde_bencode::from_bytes(bytes).unwrap()
    }
    pub fn get_info_hash(&self) -> [u8; 20] {
        let info_bytes = serde_bencode::to_bytes(&self.info).unwrap();
        let mut hasher = Sha1::new();
        hasher.update(&info_bytes);
        let info_hash = hasher.finalize();
        info_hash.into()
    }
    pub fn number_of_pieces(&self) -> u32 {
        // div_ceil not supported by Rust version of codecrafters.io:
        (self.info.length + self.info.piece_length - 1) / self.info.piece_length
    }

    pub fn piece_length(&self, piece_index: u32) -> u32 {
        if piece_index == self.number_of_pieces() - 1 {
            self.info.length % self.info.piece_length
        } else {
            self.info.piece_length
        }
    }

    pub fn number_of_blocks(&self, piece_index: u32) -> u32 {
        let block_size_u32: u32 = BLOCK_SIZE.try_into().unwrap();
        // div_ceil not supported by Rust version of codecrafters.io:
        (self.piece_length(piece_index) + block_size_u32 - 1) / block_size_u32
    }

    pub fn block_size(&self, block_index: u32, piece_index: u32) -> u32 {
        let block_size_u32 = BLOCK_SIZE.try_into().unwrap();
        let number_of_blocks = self.number_of_blocks(piece_index);
        let piece_length = self.piece_length(piece_index);
        if block_index == number_of_blocks - 1 && piece_length % block_size_u32 != 0 {
            piece_length % block_size_u32
        } else {
            block_size_u32
        }
    }

    pub fn is_piece_hash_correct(&self, piece: &[u8], piece_index: u32) -> bool {
        let a: usize = 20 * usize::try_from(piece_index).unwrap();
        let b: usize = 20 * usize::try_from(piece_index + 1).unwrap();
        let hash = |bytes: &[u8]| -> [u8; 20] {
            let mut hasher = Sha1::new();
            hasher.update(bytes);
            hasher.finalize().into()
        };
        self.info.pieces[a..b] == hash(piece)
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct Info {
    pub length: u32,
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    #[serde_as(as = "Bytes")]
    pub pieces: Vec<u8>,
}

// 16kiB
const BLOCK_SIZE: usize = 16 * 1024;
