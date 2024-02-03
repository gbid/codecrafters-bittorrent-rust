use std::io;
use tokio::io::{ AsyncReadExt, AsyncWriteExt };

#[derive(Debug)]
pub enum PeerMessage {
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
    // TODO: read from &[u8] to get rid of networking
    // Can we make this generic with the Read trait?
    pub async fn from_reader<R: AsyncReadExt + Unpin >(mut reader: R) -> io::Result<PeerMessage> {
        // read length prefix (4 bytes)
        let mut length_buf = [0u8; 4];
        reader.read_exact(&mut length_buf).await?;
        let length = u32::from_be_bytes(length_buf);
        // read message id (1 byte)
        let mut id_buf = [0u8; 1];
        reader.read_exact(&mut id_buf).await?;
        let id = u8::from_be_bytes(id_buf);
        // read payload (of length as indicated in prefix bytes)
        let payload_length: usize = <u32 as TryInto<usize>>::try_into(length).unwrap() - 1;
        let mut payload_buf: Vec<u8> = vec![0; payload_length];
        reader.read_exact(&mut payload_buf).await?;
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

    pub fn _to_bytes(&self) -> io::Result<Vec<u8>> {
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

    pub async fn write_to<W: AsyncWriteExt + Unpin>(&self, mut writer: W) -> io::Result<()> {
        match self {
            PeerMessage::Bitfield(_payload_buf) => {
                todo!()
            },
            PeerMessage::Interested => {
                let length: u32 = 1;
                let id: u8 = PeerMessage::ID_INTERESTED;
                writer.write_u32(length).await?;
                writer.write_u8(id).await?;
            },
            PeerMessage::Unchoke => {
                todo!()
            },
            PeerMessage::Request(request_payload) => {
                let length: u32 = 1 + 3*4;
                let id: u8 = PeerMessage::ID_REQUEST;
                writer.write_u32(length).await?;
                writer.write_u8(id).await?;
                writer.write_u32(request_payload.index).await?;
                writer.write_u32(request_payload.begin).await?;
                writer.write_u32(request_payload.length).await?;
            },
            PeerMessage::Piece(_piece_payload) => {
                todo!()
            },
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct RequestPayload {
    pub index: u32,
    pub begin: u32,
    pub length: u32,
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
pub struct PiecePayload {
    pub index: u32,
    pub begin: u32,
    pub block: Vec<u8>,
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
