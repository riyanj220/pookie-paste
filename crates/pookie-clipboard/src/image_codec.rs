use std::fmt;
use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits};

///
/// Maximum width or height accepted from clipboard image input.
///
/// This protects Pookie from malformed or hostile images that
/// advertise absurd dimensions before decoding.
///
pub const MAX_IMAGE_DIMENSION: u32 = 16_384;

///
/// Maximum number of decoded pixels accepted by Pookie.
///
/// 40 million pixels comfortably covers an 8K image while
/// preventing extremely large decoded clipboard images from
/// consuming unreasonable amounts of memory.
///
pub const MAX_IMAGE_PIXELS: u64 = 40_000_000;

///
/// Best-effort decoder allocation ceiling.
///
/// The strict width/height checks and explicit pixel-count
/// check are the primary protections. This gives compatible
/// decoders an additional allocation guard.
///
pub const MAX_DECODE_ALLOCATION: u64 = 256 * 1024 * 1024;

///
/// MIME types accepted as normal clipboard images.
///
/// Order is also the preferred selection order when an
/// application offers multiple representations of the same
/// clipboard image.
///
/// Pookie prefers PNG because it already matches the
/// canonical internal representation.
///
pub const SUPPORTED_IMAGE_MIME_TYPES: &[&str] = &[
    "image/png",
    "image/x-png",
    "image/jpeg",
    "image/jpg",
    "image/pjpeg",
    "image/webp",
    "image/bmp",
    "image/x-bmp",
    "image/x-ms-bmp",
    "image/gif",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageCodecError {
    EmptyInput,

    UnsupportedMimeType(String),

    InvalidDimensions {
        width: u32,
        height: u32,
    },

    DimensionLimitExceeded {
        width: u32,
        height: u32,
        maximum: u32,
    },

    PixelLimitExceeded {
        width: u32,
        height: u32,
        pixels: u64,
        maximum: u64,
    },

    DimensionReadFailed(String),

    DecodeFailed(String),

    EncodeFailed(String),
}

impl fmt::Display for ImageCodecError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => {
                write!(formatter, "clipboard image is empty")
            }

            Self::UnsupportedMimeType(mime) => {
                write!(formatter, "unsupported clipboard image MIME type: {mime}")
            }

            Self::InvalidDimensions { width, height } => {
                write!(
                    formatter,
                    "clipboard image has invalid dimensions: {width}x{height}"
                )
            }

            Self::DimensionLimitExceeded {
                width,
                height,
                maximum,
            } => {
                write!(
                    formatter,
                    "clipboard image dimensions {width}x{height} exceed \
                     the maximum dimension of {maximum}px"
                )
            }

            Self::PixelLimitExceeded {
                width,
                height,
                pixels,
                maximum,
            } => {
                write!(
                    formatter,
                    "clipboard image dimensions {width}x{height} contain \
                     {pixels} pixels, exceeding the maximum of {maximum}"
                )
            }

            Self::DimensionReadFailed(error) => {
                write!(
                    formatter,
                    "failed reading clipboard image dimensions: {error}"
                )
            }

            Self::DecodeFailed(error) => {
                write!(formatter, "failed decoding clipboard image: {error}")
            }

            Self::EncodeFailed(error) => {
                write!(
                    formatter,
                    "failed encoding canonical clipboard image: {error}"
                )
            }
        }
    }
}

impl std::error::Error for ImageCodecError {}

///
/// Convert an application-provided clipboard image into
/// Pookie's canonical image representation.
///
/// Canonical representation:
///
/// PNG-encoded RGBA8 bytes
///
/// All supported input formats are decoded into pixels and
/// re-encoded as RGBA8 PNG.
///
/// This means `ClipboardContent::Image(Vec<u8>)` can have one
/// predictable meaning throughout Pookie.
///
/// The caller is still responsible for applying Pookie's
/// normal ClipboardPolicy afterwards. In particular, the
/// existing MAX_IMAGE_SIZE policy remains the single source
/// of truth for the maximum canonical clipboard payload size.
///
pub fn canonicalize_image(encoded: &[u8], mime_type: &str) -> Result<Vec<u8>, ImageCodecError> {
    if encoded.is_empty() {
        return Err(ImageCodecError::EmptyInput);
    }

    let format = image_format_for_mime(mime_type)
        .ok_or_else(|| ImageCodecError::UnsupportedMimeType(mime_type.to_string()))?;

    /*
     * Read dimensions before decoding the complete image.
     *
     * This prevents obviously unreasonable images from
     * causing a large decoded allocation.
     */
    let (width, height) = ImageReader::with_format(Cursor::new(encoded), format)
        .into_dimensions()
        .map_err(|error| ImageCodecError::DimensionReadFailed(error.to_string()))?;

    validate_dimensions(width, height)?;

    /*
     * Apply decoder-level limits as a second layer.
     *
     * max_image_width / max_image_height are strict limits
     * in the image crate.
     *
     * max_alloc is an additional best-effort allocation
     * limit because not every decoder can enforce it
     * identically.
     */
    let mut limits = Limits::default();

    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);
    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);

    let mut reader = ImageReader::with_format(Cursor::new(encoded), format);

    reader.limits(limits);

    let decoded = reader
        .decode()
        .map_err(|error| ImageCodecError::DecodeFailed(error.to_string()))?;

    /*
     * Force every supported source format into exactly the
     * same pixel representation before PNG encoding.
     *
     * Without this step, one image could remain RGB while
     * another could remain RGBA, producing different
     * canonical PNG bytes even when their decoded pixels
     * otherwise represent the same content.
     */
    let rgba = decoded.to_rgba8();

    encode_canonical_png(DynamicImage::ImageRgba8(rgba))
}

pub fn is_supported_image_mime(mime_type: &str) -> bool {
    image_format_for_mime(mime_type).is_some()
}

///
/// Pick Pookie's preferred image MIME type from an offered
/// clipboard MIME list.
///
/// The returned value is the exact MIME string supplied by
/// the clipboard owner, not one of Pookie's static strings.
///
/// This matters because Wayland receive requests must use
/// the MIME type exactly as it was offered.
///
pub fn preferred_image_mime(offered: &[String]) -> Option<&str> {
    for preferred in SUPPORTED_IMAGE_MIME_TYPES {
        if let Some(value) = offered
            .iter()
            .find(|offered_mime| mime_matches(offered_mime, preferred))
        {
            return Some(value.as_str());
        }
    }

    None
}

fn image_format_for_mime(mime_type: &str) -> Option<ImageFormat> {
    match normalized_mime(mime_type).as_str() {
        "image/png" | "image/x-png" => Some(ImageFormat::Png),

        "image/jpeg" | "image/jpg" | "image/pjpeg" => Some(ImageFormat::Jpeg),

        "image/webp" => Some(ImageFormat::WebP),

        "image/bmp" | "image/x-bmp" | "image/x-ms-bmp" => Some(ImageFormat::Bmp),

        "image/gif" => Some(ImageFormat::Gif),

        _ => None,
    }
}

fn normalized_mime(mime_type: &str) -> String {
    mime_type
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn mime_matches(offered: &str, preferred: &str) -> bool {
    normalized_mime(offered) == preferred
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), ImageCodecError> {
    if width == 0 || height == 0 {
        return Err(ImageCodecError::InvalidDimensions { width, height });
    }

    if width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(ImageCodecError::DimensionLimitExceeded {
            width,
            height,
            maximum: MAX_IMAGE_DIMENSION,
        });
    }

    let pixels = u64::from(width).checked_mul(u64::from(height)).ok_or(
        ImageCodecError::PixelLimitExceeded {
            width,
            height,
            pixels: u64::MAX,
            maximum: MAX_IMAGE_PIXELS,
        },
    )?;

    if pixels > MAX_IMAGE_PIXELS {
        return Err(ImageCodecError::PixelLimitExceeded {
            width,
            height,
            pixels,
            maximum: MAX_IMAGE_PIXELS,
        });
    }

    Ok(())
}

fn encode_canonical_png(image: DynamicImage) -> Result<Vec<u8>, ImageCodecError> {
    let rgba = image.to_rgba8();

    let width = rgba.width();
    let height = rgba.height();

    /*
     * The decoded image already passed validation, but keep
     * the invariant local to the encoder as well.
     */
    validate_dimensions(width, height)?;

    let mut output = Vec::new();

    PngEncoder::new(&mut output)
        .write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)
        .map_err(|error| ImageCodecError::EncodeFailed(error.to_string()))?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, ImageReader, Rgba, RgbaImage};

    use super::{
        ImageCodecError, MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS, canonicalize_image,
        is_supported_image_mime, preferred_image_mime, validate_dimensions,
    };

    fn sample_image() -> DynamicImage {
        let image = RgbaImage::from_fn(4, 3, |x, y| {
            Rgba([(x * 40) as u8, (y * 60) as u8, ((x + y) * 25) as u8, 255])
        });

        DynamicImage::ImageRgba8(image)
    }

    fn encode_test_image(format: ImageFormat) -> Vec<u8> {
        let image = sample_image();

        let mut bytes = Vec::new();

        image
            .write_to(&mut Cursor::new(&mut bytes), format)
            .expect("failed encoding test image");

        bytes
    }

    #[test]
    fn recognizes_supported_image_mime_types() {
        assert!(is_supported_image_mime("image/png"));
        assert!(is_supported_image_mime("image/jpeg"));
        assert!(is_supported_image_mime("image/jpg"));
        assert!(is_supported_image_mime("image/webp"));
        assert!(is_supported_image_mime("image/bmp"));
        assert!(is_supported_image_mime("image/gif"));
    }

    #[test]
    fn mime_matching_is_case_insensitive_and_ignores_parameters() {
        assert!(is_supported_image_mime("IMAGE/PNG; charset=binary",));

        assert!(is_supported_image_mime(" image/jpeg ",));
    }

    #[test]
    fn rejects_unsupported_image_mime_type() {
        let result = canonicalize_image(&[1, 2, 3], "image/tiff");

        assert!(matches!(
            result,
            Err(ImageCodecError::UnsupportedMimeType(_))
        ));
    }

    #[test]
    fn rejects_empty_image_payload() {
        let result = canonicalize_image(&[], "image/png");

        assert_eq!(result, Err(ImageCodecError::EmptyInput),);
    }

    #[test]
    fn canonicalizes_png_image() {
        let source = encode_test_image(ImageFormat::Png);

        let canonical =
            canonicalize_image(&source, "image/png").expect("PNG canonicalization failed");

        let decoded = ImageReader::with_format(Cursor::new(&canonical), ImageFormat::Png)
            .decode()
            .expect("canonical PNG failed to decode");

        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 3);
        assert_eq!(decoded.to_rgba8(), sample_image().to_rgba8(),);
    }

    #[test]
    fn canonicalizes_jpeg_image() {
        let source = encode_test_image(ImageFormat::Jpeg);

        let canonical =
            canonicalize_image(&source, "image/jpeg").expect("JPEG canonicalization failed");

        let decoded = ImageReader::with_format(Cursor::new(&canonical), ImageFormat::Png)
            .decode()
            .expect("canonical JPEG-derived PNG failed to decode");

        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 3);
    }

    #[test]
    fn canonicalizes_webp_image() {
        let source = encode_test_image(ImageFormat::WebP);

        let canonical =
            canonicalize_image(&source, "image/webp").expect("WebP canonicalization failed");

        let decoded = ImageReader::with_format(Cursor::new(&canonical), ImageFormat::Png)
            .decode()
            .expect("canonical WebP-derived PNG failed to decode");

        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 3);
    }

    #[test]
    fn canonicalizes_bmp_image() {
        let source = encode_test_image(ImageFormat::Bmp);

        let canonical =
            canonicalize_image(&source, "image/bmp").expect("BMP canonicalization failed");

        let decoded = ImageReader::with_format(Cursor::new(&canonical), ImageFormat::Png)
            .decode()
            .expect("canonical BMP-derived PNG failed to decode");

        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 3);
    }

    #[test]
    fn canonicalizes_gif_image() {
        let source = encode_test_image(ImageFormat::Gif);

        let canonical =
            canonicalize_image(&source, "image/gif").expect("GIF canonicalization failed");

        let decoded = ImageReader::with_format(Cursor::new(&canonical), ImageFormat::Png)
            .decode()
            .expect("canonical GIF-derived PNG failed to decode");

        assert_eq!(decoded.width(), 4);
        assert_eq!(decoded.height(), 3);
    }

    #[test]
    fn lossless_encodings_of_same_pixels_have_same_canonical_bytes() {
        let png = encode_test_image(ImageFormat::Png);

        let bmp = encode_test_image(ImageFormat::Bmp);

        let canonical_png =
            canonicalize_image(&png, "image/png").expect("PNG canonicalization failed");

        let canonical_bmp =
            canonicalize_image(&bmp, "image/bmp").expect("BMP canonicalization failed");

        assert_eq!(canonical_png, canonical_bmp);
    }

    #[test]
    fn rejects_dimension_above_limit() {
        let result = validate_dimensions(MAX_IMAGE_DIMENSION + 1, 1);

        assert!(matches!(
            result,
            Err(ImageCodecError::DimensionLimitExceeded { .. })
        ));
    }

    #[test]
    fn rejects_excessive_pixel_count() {
        let width = 8_000;
        let height = 8_000;

        assert!(u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS);

        let result = validate_dimensions(width, height);

        assert!(matches!(
            result,
            Err(ImageCodecError::PixelLimitExceeded { .. })
        ));
    }

    #[test]
    fn accepts_normal_8k_image_dimensions() {
        let result = validate_dimensions(7_680, 4_320);

        assert!(result.is_ok());
    }

    #[test]
    fn prefers_png_when_multiple_image_formats_are_offered() {
        let offered = vec![
            "image/jpeg".to_string(),
            "image/webp".to_string(),
            "image/png".to_string(),
        ];

        assert_eq!(preferred_image_mime(&offered), Some("image/png"),);
    }

    #[test]
    fn returns_exact_offered_mime_value() {
        let offered = vec!["IMAGE/PNG; charset=binary".to_string()];

        assert_eq!(
            preferred_image_mime(&offered),
            Some("IMAGE/PNG; charset=binary"),
        );
    }
}
