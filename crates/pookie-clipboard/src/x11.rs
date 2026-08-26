use crate::{
    ClipboardBackend,
    ClipboardError,
};


pub struct X11Clipboard {

}

impl X11Clipboard {


    pub fn new() -> Result<Self, ClipboardError> {

        Ok(Self {})

    }


    pub fn name(&self) -> &'static str {

        "X11"

    }

}

impl ClipboardBackend for X11Clipboard {


    fn read(
        &self,
    ) -> Result<String, ClipboardError> {


        let mut clipboard =
            arboard::Clipboard::new()
                .map_err(|error| {

                    ClipboardError::ReadFailed(
                        error.to_string()
                    )

                })?;


        let text =
            clipboard
                .get_text()
                .map_err(|error| {

                    ClipboardError::ReadFailed(
                        error.to_string()
                    )

                })?;


        Ok(text)

    }



    fn write(
        &self,
        content: &str,
    ) -> Result<(), ClipboardError> {


        let mut clipboard =
            arboard::Clipboard::new()
                .map_err(|error| {

                    ClipboardError::WriteFailed(
                        error.to_string()
                    )

                })?;



        clipboard
            .set_text(content)
            .map_err(|error| {

                ClipboardError::WriteFailed(
                    error.to_string()
                )

            })?;


        Ok(())

    }

}