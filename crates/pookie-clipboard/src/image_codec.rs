use std::fmt;
use std::io::Cursor;

use image::codecs::png::PngEncoder;
use image::{
    DynamicImage, ExtendedColorType, ImageEncoder, ImageFormat, ImageReader, Limits, RgbaImage,
};
use sha2::{Digest, Sha256};

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

///
/// Domain separator for version 1 of Pookie's RGBA pixel identity hashing.
///
pub const RGBA_V1_DOMAIN_SEPARATOR: &[u8] = b"pookie-image-rgba-v1\0";

///
/// Typed 32-byte cryptographic identity for image content.
///
/// Computed as:
/// ```text
/// SHA256(
///     b"pookie-image-rgba-v1\0"
///     || width.to_be_bytes()
///     || height.to_be_bytes()
///     || straight RGBA8 row-major bytes
/// )
/// ```
///
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageIdentity([u8; 32]);

impl ImageIdentity {
    #[allow(dead_code)]
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn into_bytes(self) -> [u8; 32] {
        self.0
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    pub fn to_versioned_string(&self) -> String {
        format!("rgba-v1:{}", self.to_hex())
    }
}

///
/// Canonical internal representation of a clipboard image.
///
/// Owns the canonical PNG-encoded bytes and the stable `ImageIdentity`
/// computed from the decoded RGBA pixels before compression.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalImage {
    png_bytes: Vec<u8>,
    identity: ImageIdentity,
}

impl CanonicalImage {
    pub(crate) fn new(png_bytes: Vec<u8>, identity: ImageIdentity) -> Self {
        Self {
            png_bytes,
            identity,
        }
    }

    pub fn png_bytes(&self) -> &[u8] {
        &self.png_bytes
    }

    pub fn into_png_bytes(self) -> Vec<u8> {
        self.png_bytes
    }

    pub fn identity(&self) -> ImageIdentity {
        self.identity
    }

    pub fn len(&self) -> usize {
        self.png_bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.png_bytes.is_empty()
    }
}

///
/// Compute the stable `rgba-v1` identity from raw RGBA8 pixels and dimensions.
///
/// Streams straight RGBA8 row-major bytes directly into SHA-256 without
/// unnecessary buffer copies.
///
pub fn compute_image_identity(width: u32, height: u32, rgba: &[u8]) -> ImageIdentity {
    let mut hasher = Sha256::new();
    hasher.update(RGBA_V1_DOMAIN_SEPARATOR);
    hasher.update(width.to_be_bytes());
    hasher.update(height.to_be_bytes());
    hasher.update(rgba);

    ImageIdentity(hasher.finalize().into())
}

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

    InvalidRgbaLength {
        width: u32,
        height: u32,
        expected: u64,
        actual: usize,
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

            Self::InvalidRgbaLength {
                width,
                height,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "clipboard RGBA image {width}x{height} requires \
                     {expected} bytes but received {actual}"
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
/// Convert an application-provided encoded clipboard image
/// into Pookie's canonical representation.
///
/// Canonical representation:
///
/// ```text
/// PNG-encoded RGBA8 bytes
/// ```
///
pub fn canonicalize_image(
    encoded: &[u8],
    mime_type: &str,
) -> Result<CanonicalImage, ImageCodecError> {
    if encoded.is_empty() {
        return Err(ImageCodecError::EmptyInput);
    }

    let format = image_format_for_mime(mime_type)
        .ok_or_else(|| ImageCodecError::UnsupportedMimeType(mime_type.to_string()))?;

    canonicalize_image_format(encoded, format)
}

///
/// Check if an `image::ImageFormat` is among Pookie's supported formats.
///
pub(crate) fn is_supported_image_format(format: ImageFormat) -> bool {
    matches!(
        format,
        ImageFormat::Png
            | ImageFormat::Jpeg
            | ImageFormat::WebP
            | ImageFormat::Bmp
            | ImageFormat::Gif
    )
}

///
/// Convert an encoded image of known format into Pookie's canonical representation.
///
/// Canonical representation:
///
/// ```text
/// PNG-encoded RGBA8 bytes + ImageIdentity
/// ```
///
pub(crate) fn canonicalize_image_format(
    encoded: &[u8],
    format: ImageFormat,
) -> Result<CanonicalImage, ImageCodecError> {
    if encoded.is_empty() {
        return Err(ImageCodecError::EmptyInput);
    }

    if !is_supported_image_format(format) {
        return Err(ImageCodecError::UnsupportedMimeType(format!("{format:?}")));
    }

    let decoded = decode_encoded_image(encoded, format)?;

    encode_canonical_png(decoded.to_rgba8())
}

///
/// Detect the image format from raw bytes and convert into Pookie's
/// canonical representation.
///
pub(crate) fn canonicalize_detected_image(
    encoded: &[u8],
) -> Result<CanonicalImage, ImageCodecError> {
    if encoded.is_empty() {
        return Err(ImageCodecError::EmptyInput);
    }

    let format = ImageReader::new(Cursor::new(encoded))
        .with_guessed_format()
        .map_err(|error| ImageCodecError::DecodeFailed(error.to_string()))?
        .format()
        .ok_or_else(|| ImageCodecError::UnsupportedMimeType("unknown/undetected".to_string()))?;

    canonicalize_image_format(encoded, format)
}

///
/// Convert raw RGBA8 pixels into Pookie's canonical PNG and ImageIdentity.
///
/// This is primarily used by X11 because `arboard` returns
/// decoded RGBA pixels rather than the original encoded
/// clipboard payload.
///
pub fn canonicalize_rgba(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<CanonicalImage, ImageCodecError> {
    validate_dimensions(width, height)?;

    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ImageCodecError::InvalidRgbaLength {
            width,
            height,
            expected: u64::MAX,
            actual: rgba.len(),
        })?;

    if rgba.len() as u64 != expected {
        return Err(ImageCodecError::InvalidRgbaLength {
            width,
            height,
            expected,
            actual: rgba.len(),
        });
    }

    let identity = compute_image_identity(width, height, rgba);

    let mut output = Vec::new();

    PngEncoder::new(&mut output)
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|error| ImageCodecError::EncodeFailed(error.to_string()))?;

    Ok(CanonicalImage::new(output, identity))
}

///
/// Decode a canonical Pookie PNG back into RGBA8 pixels.
///
/// X11 uses this when an image history item is written back
/// through `arboard`.
///
/// Returns:
///
/// ```text
/// (width, height, rgba8 pixels)
/// ```
///
pub fn decode_canonical_png_to_rgba(
    encoded: &[u8],
) -> Result<(u32, u32, Vec<u8>), ImageCodecError> {
    if encoded.is_empty() {
        return Err(ImageCodecError::EmptyInput);
    }

    let decoded = decode_encoded_image(encoded, ImageFormat::Png)?;

    let rgba = decoded.to_rgba8();

    let width = rgba.width();

    let height = rgba.height();

    Ok((width, height, rgba.into_raw()))
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

fn decode_encoded_image(
    encoded: &[u8],
    format: ImageFormat,
) -> Result<DynamicImage, ImageCodecError> {
    /*
     * Read dimensions before decoding the complete image.
     *
     * This rejects unreasonable dimensions before a large
     * decoded pixel allocation can occur.
     */
    let (width, height) = ImageReader::with_format(Cursor::new(encoded), format)
        .into_dimensions()
        .map_err(|error| ImageCodecError::DimensionReadFailed(error.to_string()))?;

    validate_dimensions(width, height)?;

    let mut limits = Limits::default();

    limits.max_image_width = Some(MAX_IMAGE_DIMENSION);

    limits.max_image_height = Some(MAX_IMAGE_DIMENSION);

    limits.max_alloc = Some(MAX_DECODE_ALLOCATION);

    let mut reader = ImageReader::with_format(Cursor::new(encoded), format);

    reader.limits(limits);

    reader
        .decode()
        .map_err(|error| ImageCodecError::DecodeFailed(error.to_string()))
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

fn encode_canonical_png(rgba: RgbaImage) -> Result<CanonicalImage, ImageCodecError> {
    let width = rgba.width();

    let height = rgba.height();

    validate_dimensions(width, height)?;

    let identity = compute_image_identity(width, height, rgba.as_raw());

    let mut output = Vec::new();

    PngEncoder::new(&mut output)
        .write_image(rgba.as_raw(), width, height, ExtendedColorType::Rgba8)
        .map_err(|error| ImageCodecError::EncodeFailed(error.to_string()))?;

    drop(rgba);

    Ok(CanonicalImage::new(output, identity))
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, ImageReader, Rgba, RgbaImage};

    use super::{
        ImageCodecError, ImageIdentity, MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS,
        canonicalize_detected_image, canonicalize_image, canonicalize_image_format,
        canonicalize_rgba, compute_image_identity, decode_canonical_png_to_rgba,
        is_supported_image_format, is_supported_image_mime, preferred_image_mime,
        validate_dimensions,
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

        let decoded =
            ImageReader::with_format(Cursor::new(canonical.png_bytes()), ImageFormat::Png)
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

        let decoded =
            ImageReader::with_format(Cursor::new(canonical.png_bytes()), ImageFormat::Png)
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

        let decoded =
            ImageReader::with_format(Cursor::new(canonical.png_bytes()), ImageFormat::Png)
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

        let decoded =
            ImageReader::with_format(Cursor::new(canonical.png_bytes()), ImageFormat::Png)
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

        let decoded =
            ImageReader::with_format(Cursor::new(canonical.png_bytes()), ImageFormat::Png)
                .decode()
                .expect("canonical GIF-derived PNG failed to decode");

        assert_eq!(decoded.width(), 4);

        assert_eq!(decoded.height(), 3);
    }

    #[test]
    fn canonicalizes_raw_rgba_pixels() {
        let expected = sample_image().to_rgba8();

        let canonical = canonicalize_rgba(expected.width(), expected.height(), expected.as_raw())
            .expect("RGBA canonicalization failed");

        let (width, height, actual) = decode_canonical_png_to_rgba(canonical.png_bytes())
            .expect("canonical PNG decode failed");

        assert_eq!(width, expected.width(),);

        assert_eq!(height, expected.height(),);

        assert_eq!(actual, expected.into_raw(),);
    }

    #[test]
    fn rejects_invalid_rgba_length() {
        let result = canonicalize_rgba(2, 2, &[1, 2, 3]);

        assert!(matches!(
            result,
            Err(ImageCodecError::InvalidRgbaLength { .. })
        ));
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
        assert_eq!(canonical_png.identity(), canonical_bmp.identity());
    }

    #[test]
    fn golden_rgba_v1_vector() {
        let rgba = [
            255, 0, 0, 255, // (0,0) Red
            0, 255, 0, 255, // (1,0) Green
            0, 0, 255, 255, // (0,1) Blue
            255, 255, 0, 128, // (1,1) Semi-transparent Yellow
        ];
        let identity = compute_image_identity(2, 2, &rgba);
        assert_eq!(
            identity.to_hex(),
            "434c6661a2c15303ccca6a7244c78b3e90b558b73da9f3fd22a2f0c1a864a8c4"
        );
        assert_eq!(
            identity.to_versioned_string(),
            "rgba-v1:434c6661a2c15303ccca6a7244c78b3e90b558b73da9f3fd22a2f0c1a864a8c4"
        );
    }

    #[test]
    fn same_rgba_and_dimensions_produce_same_identity() {
        let rgba = [10, 20, 30, 255, 40, 50, 60, 255];
        let id1 = compute_image_identity(2, 1, &rgba);
        let id2 = compute_image_identity(2, 1, &rgba);
        assert_eq!(id1, id2);
    }

    #[test]
    fn one_pixel_difference_produces_different_identity() {
        let rgba1 = [10, 20, 30, 255, 40, 50, 60, 255];
        let rgba2 = [10, 20, 31, 255, 40, 50, 60, 255];
        let id1 = compute_image_identity(2, 1, &rgba1);
        let id2 = compute_image_identity(2, 1, &rgba2);
        assert_ne!(id1, id2);
    }

    #[test]
    fn dimension_difference_produces_different_identity() {
        let rgba = [10, 20, 30, 255, 40, 50, 60, 255];
        let id_2x1 = compute_image_identity(2, 1, &rgba);
        let id_1x2 = compute_image_identity(1, 2, &rgba);
        assert_ne!(id_2x1, id_1x2);
    }

    #[test]
    fn same_pixel_content_through_different_canonicalization_inputs_produces_same_identity() {
        let png = encode_test_image(ImageFormat::Png);
        let bmp = encode_test_image(ImageFormat::Bmp);

        let canonical_png =
            canonicalize_image(&png, "image/png").expect("PNG canonicalization failed");
        let canonical_bmp =
            canonicalize_image(&bmp, "image/bmp").expect("BMP canonicalization failed");

        assert_eq!(canonical_png.identity(), canonical_bmp.identity());
        assert_eq!(
            canonical_png.identity().to_versioned_string(),
            canonical_bmp.identity().to_versioned_string()
        );
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

        assert_eq!(preferred_image_mime(&offered,), Some("image/png"),);
    }

    #[test]
    fn returns_exact_offered_mime_value() {
        let offered = vec!["IMAGE/PNG; charset=binary".to_string()];

        assert_eq!(
            preferred_image_mime(&offered,),
            Some("IMAGE/PNG; charset=binary"),
        );
    }

    #[test]
    fn identifies_supported_image_formats() {
        assert!(is_supported_image_format(ImageFormat::Png));
        assert!(is_supported_image_format(ImageFormat::Jpeg));
        assert!(is_supported_image_format(ImageFormat::WebP));
        assert!(is_supported_image_format(ImageFormat::Bmp));
        assert!(is_supported_image_format(ImageFormat::Gif));
        assert!(!is_supported_image_format(ImageFormat::Tiff));
    }

    #[test]
    fn canonicalizes_all_supported_formats_by_format_type() {
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::WebP,
            ImageFormat::Bmp,
            ImageFormat::Gif,
        ] {
            let bytes = encode_test_image(format);
            let canonical = canonicalize_image_format(&bytes, format)
                .unwrap_or_else(|err| panic!("failed for format {format:?}: {err}"));
            let (width, height, _) = decode_canonical_png_to_rgba(canonical.png_bytes()).unwrap();
            assert_eq!(width, 4);
            assert_eq!(height, 3);
        }
    }

    #[test]
    fn canonicalizes_detected_image_bytes() {
        for format in [
            ImageFormat::Png,
            ImageFormat::Jpeg,
            ImageFormat::WebP,
            ImageFormat::Bmp,
            ImageFormat::Gif,
        ] {
            let bytes = encode_test_image(format);
            let canonical = canonicalize_detected_image(&bytes)
                .unwrap_or_else(|err| panic!("detection failed for format {format:?}: {err}"));
            let (width, height, _) = decode_canonical_png_to_rgba(canonical.png_bytes()).unwrap();
            assert_eq!(width, 4);
            assert_eq!(height, 3);
        }
    }

    #[test]
    fn canonicalize_detected_image_rejects_empty_and_garbage() {
        assert_eq!(
            canonicalize_detected_image(&[]),
            Err(ImageCodecError::EmptyInput)
        );
        let garbage = b"not an image at all";
        assert!(canonicalize_detected_image(garbage).is_err());
    }

    #[test]
    fn image_identity_methods_and_formatting() {
        let raw = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let identity = ImageIdentity::from_bytes(raw);
        assert_eq!(identity.as_bytes(), &raw);
        assert_eq!(identity.into_bytes(), raw);
        assert_eq!(
            identity.to_hex(),
            "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
        );
        assert_eq!(
            identity.to_versioned_string(),
            "rgba-v1:0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20"
        );
    }
}
