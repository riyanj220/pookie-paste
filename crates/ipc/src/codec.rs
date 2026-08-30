use serde::Serialize;
use serde::de::DeserializeOwned;

#[derive(Debug)]
pub enum CodecError {
    Serialization(serde_json::Error),
    Deserialization(serde_json::Error),
    MissingFrameTerminator,
}

pub fn encode<T>(value: &T) -> Result<Vec<u8>, CodecError>
where
    T: Serialize,
{
    let mut bytes = serde_json::to_vec(value).map_err(CodecError::Serialization)?;

    bytes.push(b'\n');

    Ok(bytes)
}

pub fn decode<T>(frame: &[u8]) -> Result<T, CodecError>
where
    T: DeserializeOwned,
{
    if !frame.ends_with(b"\n") {
        return Err(CodecError::MissingFrameTerminator);
    }

    serde_json::from_slice(&frame[..frame.len() - 1]).map_err(CodecError::Deserialization)
}
