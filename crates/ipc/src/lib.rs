pub mod client;
pub mod codec;
pub mod protocol;
pub mod server;
pub mod socket_path;

pub use client::{ClientError, IpcClient};
pub use codec::{CodecError, MAX_FRAME_SIZE, decode, encode};
pub use protocol::{ActivationOutcome, HistoryItem, IpcRequest, IpcResponse};
pub use server::{IpcConnection, IpcServer, ServerError};
pub use socket_path::socket_path;
