use pookie_clipboard::{
    ImageCodecError, MAX_IMAGE_DIMENSION, MAX_IMAGE_PIXELS, canonicalize_image, canonicalize_rgba,
};

#[test]
fn malformed_png_payload_is_rejected() {
    let result = canonicalize_image(b"this is not a png", "image/png");

    assert!(
        result.is_err(),
        "malformed encoded image must not be accepted",
    );

    assert!(matches!(
        result,
        Err(ImageCodecError::DimensionReadFailed(_)) | Err(ImageCodecError::DecodeFailed(_))
    ));
}

#[test]
fn unsupported_image_mime_is_rejected_before_decode() {
    let result = canonicalize_image(&[1, 2, 3, 4], "image/tiff");

    assert!(matches!(
        result,
        Err(ImageCodecError::UnsupportedMimeType(_))
    ));
}

#[test]
fn empty_image_payload_is_rejected() {
    let result = canonicalize_image(&[], "image/png");

    assert_eq!(result, Err(ImageCodecError::EmptyInput,),);
}

#[test]
fn absurd_single_dimension_is_rejected_before_rgba_allocation() {
    /*
     * Empty pixel data is intentional.
     *
     * Dimension validation must fail before Pookie ever
     * considers allocating or validating the RGBA payload.
     */
    let result = canonicalize_rgba(MAX_IMAGE_DIMENSION + 1, 1, &[]);

    assert!(matches!(
        result,
        Err(ImageCodecError::DimensionLimitExceeded { .. })
    ));
}

#[test]
fn absurd_pixel_count_is_rejected_before_rgba_allocation() {
    let width = 8_000;

    let height = 8_000;

    assert!(u64::from(width,) * u64::from(height,) > MAX_IMAGE_PIXELS,);

    /*
     * Again, no huge test allocation is needed.
     *
     * Pixel-count validation happens before the RGBA length
     * check.
     */
    let result = canonicalize_rgba(width, height, &[]);

    assert!(matches!(
        result,
        Err(ImageCodecError::PixelLimitExceeded { .. })
    ));
}

#[test]
fn malformed_rgba_length_is_rejected() {
    let result = canonicalize_rgba(2, 2, &[1, 2, 3]);

    assert!(matches!(
        result,
        Err(ImageCodecError::InvalidRgbaLength { .. })
    ));
}

#[test]
fn canonicalization_is_deterministic_for_identical_rgba_input() {
    let rgba = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];

    let first = canonicalize_rgba(2, 2, &rgba).expect("first canonicalization failed");

    let second = canonicalize_rgba(2, 2, &rgba).expect("second canonicalization failed");

    assert_eq!(
        first, second,
        "same canonical image must always produce identical PNG bytes",
    );
}
