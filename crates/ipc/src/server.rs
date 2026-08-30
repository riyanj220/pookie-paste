use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::{CodecError, IpcRequest, IpcResponse, decode, encode};

#[derive(Debug)]
pub enum ServerError {
    Io(io::Error),
    Codec(CodecError),
    ConnectionClosed,
}

pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
}

impl IpcServer {
    pub fn bind(path: impl AsRef<Path>) -> Result<Self, ServerError> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(ServerError::Io)?;
        }

        if path.exists() {
            fs::remove_file(path).map_err(ServerError::Io)?;
        }

        let listener = UnixListener::bind(path).map_err(ServerError::Io)?;

        Ok(Self {
            listener,
            socket_path: path.to_path_buf(),
        })
    }

    pub async fn accept(&self) -> Result<IpcConnection, ServerError> {
        let (stream, _) = self.listener.accept().await.map_err(ServerError::Io)?;

        Ok(IpcConnection::new(stream))
    }
}

pub struct IpcConnection {
    stream: BufReader<UnixStream>,
}

impl IpcConnection {
    fn new(stream: UnixStream) -> Self {
        Self {
            stream: BufReader::new(stream),
        }
    }

    pub async fn read_request(&mut self) -> Result<IpcRequest, ServerError> {
        let mut frame = Vec::new();

        let bytes_read = self
            .stream
            .read_until(b'\n', &mut frame)
            .await
            .map_err(ServerError::Io)?;

        if bytes_read == 0 {
            return Err(ServerError::ConnectionClosed);
        }

        decode(&frame).map_err(ServerError::Codec)
    }

    pub async fn send_response(&mut self, response: &IpcResponse) -> Result<(), ServerError> {
        let frame = encode(response).map_err(ServerError::Codec)?;

        self.stream
            .get_mut()
            .write_all(&frame)
            .await
            .map_err(ServerError::Io)?;

        self.stream
            .get_mut()
            .flush()
            .await
            .map_err(ServerError::Io)?;

        Ok(())
    }
}

impl Drop for IpcServer {
    fn drop(&mut self) {
        if self.socket_path.exists() {
            let _ = fs::remove_file(&self.socket_path);
        }
    }
}
