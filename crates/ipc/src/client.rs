use std::io;
use std::path::Path;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::{IpcRequest, IpcResponse, decode, encode};

pub struct IpcClient {
    stream: BufReader<UnixStream>,
}

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Codec(crate::CodecError),
    ConnectionClosed,
}

impl IpcClient {
    pub async fn connect(path: impl AsRef<Path>) -> io::Result<Self> {
        let stream = UnixStream::connect(path).await?;

        Ok(Self {
            stream: BufReader::new(stream),
        })
    }

    pub async fn send(&mut self, request: &IpcRequest) -> Result<IpcResponse, ClientError> {
        let frame = encode(request).map_err(ClientError::Codec)?;

        self.stream
            .get_mut()
            .write_all(&frame)
            .await
            .map_err(ClientError::Io)?;

        self.stream
            .get_mut()
            .flush()
            .await
            .map_err(ClientError::Io)?;

        let mut response_frame = Vec::new();

        let bytes_read = self
            .stream
            .read_until(b'\n', &mut response_frame)
            .await
            .map_err(ClientError::Io)?;

        if bytes_read == 0 {
            return Err(ClientError::ConnectionClosed);
        }

        decode(&response_frame).map_err(ClientError::Codec)
    }
}
