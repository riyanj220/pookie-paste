use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::path::Path;

use crate::image_codec::{self, ImageCodecError};
use crate::uri_list::{self, UriListError};

///
/// Maximum byte ceiling for reading a local source image file from disk.
///
/// Set to 32 MiB (33,554,432 bytes).
///
/// Rationale:
/// - Uncompressed BMP or high-resolution photos (e.g. 24–48 megapixel JPEG/WebP)
///   can legitimately exceed 10–20 MiB on disk while still decoding comfortably
///   within Pookie's 40-megapixel ceiling (`MAX_IMAGE_PIXELS`) and 256 MiB
///   decoder allocation ceiling (`MAX_DECODE_ALLOCATION`).
/// - Comfortably bounds I/O and memory consumption, instantly rejecting large
///   media files (videos, disk images, archives, database files).
/// - Deliberately distinct from `ClipboardPolicy::MAX_IMAGE_SIZE` (10 MiB),
///   which governs the final compressed canonical PNG stored in SQLite / history.
///
pub const MAX_LOCAL_IMAGE_FILE_BYTES: u64 = 32 * 1024 * 1024;

///
/// Errors that can occur when resolving and reading a local image file.
///
#[derive(Debug)]
pub enum FileImageError {
    /// URI list parsing error.
    Uri(UriListError),

    /// File does not exist on disk.
    NotFound,

    /// Target path is not a regular file (e.g. directory, FIFO, socket, device).
    NotRegularFile,

    /// Source file exceeds the maximum allowed file byte ceiling.
    FileTooLarge { size: u64, max: u64 },

    /// File data is not one of the supported image formats (PNG, JPEG, WebP, BMP, GIF).
    UnsupportedImageFormat,

    /// Image decoding or canonical PNG encoding failed.
    Codec(ImageCodecError),

    /// Filesystem I/O error.
    Io(std::io::Error),
}

impl fmt::Display for FileImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uri(error) => {
                write!(formatter, "URI error: {error}")
            }

            Self::NotFound => {
                write!(formatter, "file does not exist")
            }

            Self::NotRegularFile => {
                write!(formatter, "target path is not a regular file")
            }

            Self::FileTooLarge { size, max } => {
                write!(
                    formatter,
                    "file size ({size} bytes) exceeds safety ceiling of {max} bytes"
                )
            }

            Self::UnsupportedImageFormat => {
                write!(formatter, "file does not contain a supported image format")
            }

            Self::Codec(error) => {
                write!(formatter, "image codec error: {error}")
            }

            Self::Io(error) => {
                write!(formatter, "filesystem I/O error: {error}")
            }
        }
    }
}

impl std::error::Error for FileImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Uri(error) => Some(error),
            Self::Codec(error) => Some(error),
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<UriListError> for FileImageError {
    fn from(error: UriListError) -> Self {
        Self::Uri(error)
    }
}

impl From<ImageCodecError> for FileImageError {
    fn from(error: ImageCodecError) -> Self {
        Self::Codec(error)
    }
}

///
/// Resolve a single local file from a `text/uri-list` payload, safely read its
/// bytes, verify it contains a supported image format, and canonicalize it into
/// a canonical PNG.
///
pub(crate) fn canonicalize_single_local_file_uri(
    payload: &[u8],
) -> Result<Vec<u8>, FileImageError> {
    let path = uri_list::parse_single_local_file_uri(payload)?;
    read_and_canonicalize_file(&path)
}

///
/// Safely read a local file from disk and convert it into Pookie's canonical PNG.
///
pub(crate) fn read_and_canonicalize_file(path: &Path) -> Result<Vec<u8>, FileImageError> {
    read_and_canonicalize_file_with_ceiling(path, MAX_LOCAL_IMAGE_FILE_BYTES)
}

///
/// Safely read a local file with an explicit byte ceiling and convert to canonical PNG.
///
pub(crate) fn read_and_canonicalize_file_with_ceiling(
    path: &Path,
    max_bytes: u64,
) -> Result<Vec<u8>, FileImageError> {
    let bytes = read_local_file_safely(path, max_bytes)?;

    image_codec::canonicalize_detected_image(&bytes).map_err(|error| match error {
        ImageCodecError::UnsupportedMimeType(_) => FileImageError::UnsupportedImageFormat,
        other => FileImageError::Codec(other),
    })
}

///
/// Safely open and read a local file into a byte buffer.
///
/// Protections:
/// 1. Opened with `O_NONBLOCK` on Unix to prevent indefinite blocking if the target
///    path is a FIFO with no writer, a named pipe, or a slow/blocking character device.
/// 2. Metadata queried on the opened file descriptor (`fstat`) to prevent symlink/path
///    substitution races.
/// 3. Rejects anything that is not a regular file (`is_file()` check).
/// 4. Early rejection if metadata advertises size > `max_bytes`.
/// 5. Bounded read via `Read::take(max_bytes + 1)` ensuring no more than `max_bytes`
///    can ever be read into memory even if file size changes concurrently.
///
fn read_local_file_safely(path: &Path, max_bytes: u64) -> Result<Vec<u8>, FileImageError> {
    let mut file = open_file_safely(path)?;

    let metadata = file.metadata().map_err(FileImageError::Io)?;

    if !metadata.file_type().is_file() {
        return Err(FileImageError::NotRegularFile);
    }

    if metadata.len() > max_bytes {
        return Err(FileImageError::FileTooLarge {
            size: metadata.len(),
            max: max_bytes,
        });
    }

    let initial_capacity = std::cmp::min(metadata.len(), max_bytes) as usize;
    let mut bytes = Vec::with_capacity(initial_capacity);

    file.by_ref()
        .take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(FileImageError::Io)?;

    if bytes.len() as u64 > max_bytes {
        return Err(FileImageError::FileTooLarge {
            size: bytes.len() as u64,
            max: max_bytes,
        });
    }

    Ok(bytes)
}

#[cfg(unix)]
fn open_file_safely(path: &Path) -> Result<File, FileImageError> {
    use std::os::unix::fs::OpenOptionsExt;

    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
        .open(path)
        .map_err(|err| {
            if err.kind() == std::io::ErrorKind::NotFound {
                FileImageError::NotFound
            } else {
                FileImageError::Io(err)
            }
        })
}

#[cfg(not(unix))]
fn open_file_safely(path: &Path) -> Result<File, FileImageError> {
    OpenOptions::new().read(true).open(path).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            FileImageError::NotFound
        } else {
            FileImageError::Io(err)
        }
    })
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;

    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

    use super::{
        FileImageError, canonicalize_single_local_file_uri, read_and_canonicalize_file,
        read_and_canonicalize_file_with_ceiling,
    };
    use crate::image_codec::decode_canonical_png_to_rgba;

    struct TempTestDir {
        path: PathBuf,
    }

    impl TempTestDir {
        fn new(name: &str) -> Self {
            let unique = format!("pookie_file_img_{name}_{}", std::process::id());
            let path = std::env::temp_dir().join(unique);
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("failed creating temp test dir");
            Self { path }
        }

        fn file_path(&self, filename: &str) -> PathBuf {
            self.path.join(filename)
        }
    }

    impl Drop for TempTestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn sample_image() -> DynamicImage {
        let image = RgbaImage::from_fn(8, 6, |x, y| {
            Rgba([(x * 30) as u8, (y * 40) as u8, 120, 255])
        });
        DynamicImage::ImageRgba8(image)
    }

    fn create_test_image_file(dir: &TempTestDir, filename: &str, format: ImageFormat) -> PathBuf {
        let path = dir.file_path(filename);
        let image = sample_image();
        let mut file = File::create(&path).expect("failed creating test image file");
        image
            .write_to(&mut file, format)
            .expect("failed writing test image format");
        path
    }

    #[test]
    fn canonicalizes_valid_png_file() {
        let dir = TempTestDir::new("png");
        let path = create_test_image_file(&dir, "sample.png", ImageFormat::Png);

        let canonical = read_and_canonicalize_file(&path).expect("canonicalization failed");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }

    #[test]
    fn canonicalizes_valid_jpeg_file() {
        let dir = TempTestDir::new("jpeg");
        let path = create_test_image_file(&dir, "sample.jpg", ImageFormat::Jpeg);

        let canonical = read_and_canonicalize_file(&path).expect("canonicalization failed");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }

    #[test]
    fn canonicalizes_valid_webp_file() {
        let dir = TempTestDir::new("webp");
        let path = create_test_image_file(&dir, "sample.webp", ImageFormat::WebP);

        let canonical = read_and_canonicalize_file(&path).expect("canonicalization failed");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }

    #[test]
    fn canonicalizes_valid_bmp_file() {
        let dir = TempTestDir::new("bmp");
        let path = create_test_image_file(&dir, "sample.bmp", ImageFormat::Bmp);

        let canonical = read_and_canonicalize_file(&path).expect("canonicalization failed");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }

    #[test]
    fn canonicalizes_valid_gif_file() {
        let dir = TempTestDir::new("gif");
        let path = create_test_image_file(&dir, "sample.gif", ImageFormat::Gif);

        let canonical = read_and_canonicalize_file(&path).expect("canonicalization failed");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }

    #[test]
    fn canonicalizes_extensionless_valid_image() {
        let dir = TempTestDir::new("no_ext");
        let path = create_test_image_file(&dir, "image_without_extension", ImageFormat::Png);

        let canonical = read_and_canonicalize_file(&path).expect("canonicalization failed");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }

    #[test]
    fn rejects_fake_png_with_text_content() {
        let dir = TempTestDir::new("fake_png");
        let path = dir.file_path("fake.png");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"not a png image, just text").unwrap();

        let result = read_and_canonicalize_file(&path);
        assert!(matches!(
            result,
            Err(FileImageError::UnsupportedImageFormat)
        ));
    }

    #[test]
    fn rejects_non_image_pdf() {
        let dir = TempTestDir::new("pdf");
        let path = dir.file_path("document.pdf");
        let mut file = File::create(&path).unwrap();
        file.write_all(b"%PDF-1.4\n%fake pdf content").unwrap();

        let result = read_and_canonicalize_file(&path);
        assert!(matches!(
            result,
            Err(FileImageError::UnsupportedImageFormat)
        ));
    }

    #[test]
    fn rejects_missing_path() {
        let missing = PathBuf::from("/nonexistent/path/definitely_not_here.png");
        let result = read_and_canonicalize_file(&missing);
        assert!(matches!(result, Err(FileImageError::NotFound)));
    }

    #[test]
    fn rejects_directory() {
        let dir = TempTestDir::new("dir_check");
        let result = read_and_canonicalize_file(&dir.path);
        assert!(matches!(result, Err(FileImageError::NotRegularFile)));
    }

    #[test]
    #[cfg(unix)]
    fn safely_rejects_fifo_without_blocking() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        let dir = TempTestDir::new("fifo_check");
        let fifo_path = dir.file_path("test_pipe.fifo");
        let c_path = CString::new(fifo_path.as_os_str().as_bytes()).unwrap();

        let res = unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) };
        assert_eq!(res, 0, "mkfifo failed");

        // Opening a FIFO without O_NONBLOCK when there is no writer would hang forever.
        // Our safe implementation opens with O_NONBLOCK and checks is_file(), returning immediately.
        let result = read_and_canonicalize_file(&fifo_path);
        assert!(matches!(result, Err(FileImageError::NotRegularFile)));
    }

    #[test]
    fn rejects_file_exceeding_ceiling() {
        let dir = TempTestDir::new("size_check");
        let path = dir.file_path("large_fake.png");
        let mut file = File::create(&path).unwrap();
        file.write_all(&[0u8; 1024]).unwrap();

        // Testing with explicit ceiling of 512 bytes
        let result = read_and_canonicalize_file_with_ceiling(&path, 512);
        assert!(matches!(
            result,
            Err(FileImageError::FileTooLarge {
                size: 1024,
                max: 512
            })
        ));
    }

    #[test]
    fn canonicalizes_full_pipeline_from_uri_list_payload() {
        let dir = TempTestDir::new("full_uri");
        let image_path = create_test_image_file(&dir, "photo.png", ImageFormat::Png);

        let uri_payload = format!(
            "# Comment header\r\nfile://{}\r\n",
            image_path.to_str().unwrap()
        );

        let canonical = canonicalize_single_local_file_uri(uri_payload.as_bytes())
            .expect("pipeline failed from URI");
        let (width, height, _) = decode_canonical_png_to_rgba(&canonical).unwrap();
        assert_eq!(width, 8);
        assert_eq!(height, 6);
    }
}
