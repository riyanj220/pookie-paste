use std::fs;
use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

use crate::{CodecError, IpcRequest, IpcResponse, MAX_FRAME_SIZE, decode, encode};

#[derive(Debug)]
pub enum ServerError {
    Io(io::Error),
    Codec(CodecError),
    ConnectionClosed,

    FrameTooLarge { size: usize, max: usize },
    AlreadyRunning { path: PathBuf },
}

pub struct IpcServer {
    listener: UnixListener,
    socket_path: PathBuf,
}

fn remove_stale_socket(path: &Path) -> Result<(), ServerError> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::symlink_metadata(path).map_err(ServerError::Io)?;

    if !metadata.file_type().is_socket() {
        return Err(ServerError::Io(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "IPC path exists and is not a Unix socket: {}",
                path.display()
            ),
        )));
    }

    match std::os::unix::net::UnixStream::connect(path) {
        Ok(_) => Err(ServerError::AlreadyRunning {
            path: path.to_path_buf(),
        }),

        Err(error) if error.kind() == io::ErrorKind::ConnectionRefused => {
            fs::remove_file(path).map_err(ServerError::Io)?;

            Ok(())
        }

        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),

        Err(error) => Err(ServerError::Io(error)),
    }
}

impl IpcServer {
    pub fn bind(path: impl AsRef<Path>) -> Result<Self, ServerError> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(ServerError::Io)?;
        }

        remove_stale_socket(path)?;

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

        loop {
            let buffer = self.stream.fill_buf().await.map_err(ServerError::Io)?;

            if buffer.is_empty() {
                if frame.is_empty() {
                    return Err(ServerError::ConnectionClosed);
                }

                return Err(ServerError::Codec(CodecError::MissingFrameTerminator));
            }

            if let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
                let take = position + 1;

                if frame.len() + take > MAX_FRAME_SIZE {
                    return Err(ServerError::FrameTooLarge {
                        size: frame.len() + take,

                        max: MAX_FRAME_SIZE,
                    });
                }

                frame.extend_from_slice(&buffer[..take]);

                self.stream.consume(take);

                break;
            }

            if frame.len() + buffer.len() > MAX_FRAME_SIZE {
                return Err(ServerError::FrameTooLarge {
                    size: frame.len() + buffer.len(),

                    max: MAX_FRAME_SIZE,
                });
            }

            let consumed = buffer.len();

            frame.extend_from_slice(buffer);

            self.stream.consume(consumed);
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
