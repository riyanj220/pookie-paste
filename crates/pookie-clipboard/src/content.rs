#[derive(Debug)]
pub enum ClipboardContent {

    Text(String),

    Image(Vec<u8>),

}