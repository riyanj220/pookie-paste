use chrono::{DateTime, Utc};

use crate::ClipboardContent;


#[derive(Debug)]
pub struct ClipboardEvent {

    pub id: String,

    pub content: ClipboardContent,

    pub created_at: DateTime<Utc>,

}


impl ClipboardEvent {


    pub fn new(
        content: ClipboardContent
    ) -> Self {

        Self {

            id: uuid::Uuid::new_v4()
                .to_string(),

            content,

            created_at: Utc::now(),

        }

    }

}