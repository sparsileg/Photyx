// writer.rs — XISF file writer

use std::path::Path;
use crate::error::XisfError;
use crate::types::{WriteOptions, XisfImage};

/// Writes XISF files.
pub struct XisfWriter;

impl XisfWriter {
    /// Write a single image to an XISF file.
    ///
    /// # Example
    /// ```no_run
    /// use photyx_xisf::{XisfReader, XisfWriter, WriteOptions};
    /// let reader = XisfReader::open("input.xisf").unwrap();
    /// let image = reader.read_image(0).unwrap();
    /// XisfWriter::write("output.xisf", &image, &WriteOptions::default()).unwrap();
    /// ```

    pub fn write(
        path:    impl AsRef<Path>,
        image:   &XisfImage,
        options: &WriteOptions,
    ) -> Result<(), XisfError> {
        todo!()
    }
}
