use packet_datatypes::{Packet, PacketReader, PacketWriter, error::Result};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use std::io::{Error, ErrorKind};

pub struct FramedStream<S> {
    inner: S,
}

impl<S: AsyncRead + AsyncWrite + Unpin> FramedStream<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    pub fn into_inner(self) -> S {
        self.inner
    }

    pub fn inner_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Reads a specific packet dynamically verifying its type ID.
    pub async fn read_packet<P: Packet>(&mut self) -> Result<P> {
        let (id, payload) = self.read_any_packet().await?;
        
        if id != P::ID {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("Expected packet ID {}, but got {}", P::ID, id)
            ).into());
        }

        let mut reader = PacketReader::new(&payload);
        P::decode(&mut reader)
    }  

    /// Reads the raw framing of the packet (Length -> ID -> Payload).
    pub async fn read_any_packet(&mut self) -> Result<(u32, Vec<u8>)> {
        let length = self.inner.read_u32().await? as usize;
        
        let id = self.inner.read_u32().await?;
        
        let payload_len = length.saturating_sub(4);
        let mut buffer = vec![0; payload_len];
        
        if payload_len > 0 {
            self.inner.read_exact(&mut buffer).await?;
        }

        Ok((id, buffer))
    }

    pub async fn write_packet<P: Packet>(&mut self, packet: &P) -> Result<()> {
        let mut payload_writer = PacketWriter::new();
        packet.encode(&mut payload_writer)?;

        let length = (4 + payload_writer.len()) as u32;
        self.inner.write_u32(length).await?;
        self.inner.write_u32(P::ID).await?;
        self.inner.write_all(&payload_writer.into_inner()).await?;

        Ok(())
    }
}