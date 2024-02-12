use crate::Torrent;
use serde::Deserialize;
use serde_with::{serde_as, Bytes};
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

#[serde_as]
#[derive(Deserialize, Debug)]
struct IntermediateTrackerResponse {
    // interval: i64,
    //#[serde(rename = "min interval")]
    //min_interval: i64,
    #[serde_as(as = "Bytes")]
    peers: Vec<u8>,
    //complete: i64,
    //incomplete: i64,
}

#[derive(Debug)]
pub struct TrackerResponse {
    // interval: i64,
    pub peers: Vec<SocketAddr>,
}

impl TrackerResponse {
    fn from_intermediate(intermediate: IntermediateTrackerResponse) -> Self {
        let peers: Vec<SocketAddr> = intermediate
            .peers
            .chunks(6)
            .map(|chunk| {
                let arr: [u8; 6] = chunk.try_into().expect("Chunk must have length 6");
                SocketAddr::from_bytes(&arr)
            })
            .collect();
        TrackerResponse {
            // interval: intermediate.interval,
            peers,
        }
    }
}

pub fn get_tracker(torrent: Arc<Torrent>) -> TrackerResponse {
    let info_hash = torrent.get_info_hash();
    let info_hash_encoded = urlencoding::encode_binary(&info_hash);
    // mock our own peer_id as proposed by codecrafters.io
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
        .get(full_url)
        .send()
        .unwrap();

    let response_body = response.bytes().unwrap();
    let intermediate_tracker_response: IntermediateTrackerResponse =
        serde_bencode::from_bytes(&response_body).unwrap();
    TrackerResponse::from_intermediate(intermediate_tracker_response)
}

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
