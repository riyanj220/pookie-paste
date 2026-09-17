use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::{Component, Path, PathBuf};

use eframe::egui;

use image::{ImageFormat, ImageReader, Limits};

const APP_DIRECTORY: &str = "pookie-paste";

const IMAGE_DIRECTORY: &str = "images";

const IMAGE_EXTENSION: &str = ".png";

const MAX_SOURCE_DIMENSION: u32 = 16_384;

const MAX_SOURCE_PIXELS: u64 = 40_000_000;

const MAX_DECODE_ALLOCATION: u64 = 256 * 1024 * 1024;

/*
 * We never need the full source resolution on the GPU.
 *
 * 256 px leaves plenty of detail for Pookie's compact
 * clipboard-history preview while keeping texture memory
 * tiny.
 */
const THUMBNAIL_MAX_DIMENSION: u32 = 256;

#[derive(Debug, Clone, Copy)]
pub struct ImageThumbnail {
    pub texture_id: egui::TextureId,

    pub aspect_ratio: f32,
}

enum CacheEntry {
    Ready {
        texture: egui::TextureHandle,

        aspect_ratio: f32,
    },

    Failed,
}

///
/// Short-lived UI image cache.
///
/// The Pookie popup is itself short-lived, so there is no
/// reason to maintain persistent thumbnail files or a
/// daemon-side cache.
///
/// Each history image is decoded at most once per popup
/// process.
///
pub struct ImageThumbnailCache {
    data_directory: Result<PathBuf, String>,

    entries: HashMap<String, CacheEntry>,
}

impl ImageThumbnailCache {
    pub fn new() -> Self {
        Self {
            data_directory: data_directory(),

            entries: HashMap::new(),
        }
    }

    pub fn get_or_load(
        &mut self,
        ctx: &egui::Context,
        item_id: &str,
        relative_path: &str,
    ) -> Option<ImageThumbnail> {
        if let Some(entry) = self.entries.get(item_id) {
            return thumbnail_from_entry(entry);
        }

        let result = self.load_thumbnail(ctx, item_id, relative_path);

        match result {
            Ok((texture, aspect_ratio)) => {
                self.entries.insert(
                    item_id.to_string(),
                    CacheEntry::Ready {
                        texture,
                        aspect_ratio,
                    },
                );
            }

            Err(error) => {
                tracing::debug!(
                    item_id = %item_id,
                    path = %relative_path,
                    error = %error,
                    "failed loading clipboard image thumbnail"
                );

                self.entries.insert(item_id.to_string(), CacheEntry::Failed);
            }
        }

        self.entries.get(item_id).and_then(thumbnail_from_entry)
    }

    fn load_thumbnail(
        &self,
        ctx: &egui::Context,
        item_id: &str,
        relative_path: &str,
    ) -> Result<(egui::TextureHandle, f32), String> {
        let data_directory = self.data_directory.as_ref().map_err(Clone::clone)?;

        let relative_path = validate_image_reference(relative_path)?;

        let path = data_directory.join(relative_path);

        let (width, height) = read_dimensions(&path)?;

        validate_dimensions(width, height)?;

        let file = File::open(&path).map_err(|error| {
            format!(
                "failed opening thumbnail source {}: {error}",
                path.display(),
            )
        })?;

        let mut reader = ImageReader::with_format(BufReader::new(file), ImageFormat::Png);

        let mut limits = Limits::default();

        limits.max_image_width = Some(MAX_SOURCE_DIMENSION);

        limits.max_image_height = Some(MAX_SOURCE_DIMENSION);

        limits.max_alloc = Some(MAX_DECODE_ALLOCATION);

        reader.limits(limits);

        let decoded = reader.decode().map_err(|error| {
            format!(
                "failed decoding thumbnail source {}: {error}",
                path.display(),
            )
        })?;

        /*
         * Decode once, then immediately reduce the image
         * before creating the GPU texture.
         *
         * The original clipboard image may be 4K/8K, but
         * keeping that full resolution on the GPU would be
         * wasteful for a small popup thumbnail.
         */
        let thumbnail = decoded.thumbnail(THUMBNAIL_MAX_DIMENSION, THUMBNAIL_MAX_DIMENSION);

        let rgba = thumbnail.to_rgba8();

        let texture_width = usize::try_from(rgba.width())
            .map_err(|_| "thumbnail width exceeds platform usize".to_string())?;

        let texture_height = usize::try_from(rgba.height())
            .map_err(|_| "thumbnail height exceeds platform usize".to_string())?;

        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [texture_width, texture_height],
            rgba.as_raw(),
        );

        let texture = ctx.load_texture(
            format!("pookie-history-image-{item_id}"),
            color_image,
            egui::TextureOptions::LINEAR,
        );

        let aspect_ratio = width as f32 / height as f32;

        Ok((texture, aspect_ratio))
    }
}

impl Default for ImageThumbnailCache {
    fn default() -> Self {
        Self::new()
    }
}

fn thumbnail_from_entry(entry: &CacheEntry) -> Option<ImageThumbnail> {
    match entry {
        CacheEntry::Ready {
            texture,
            aspect_ratio,
        } => Some(ImageThumbnail {
            texture_id: texture.id(),

            aspect_ratio: *aspect_ratio,
        }),

        CacheEntry::Failed => None,
    }
}

fn data_directory() -> Result<PathBuf, String> {
    if let Some(data_home) = env::var_os("XDG_DATA_HOME")
        && !data_home.is_empty()
    {
        return Ok(PathBuf::from(data_home).join(APP_DIRECTORY));
    }

    let home = env::var_os("HOME")
        .ok_or_else(|| "neither XDG_DATA_HOME nor HOME is available".to_string())?;

    if home.is_empty() {
        return Err("HOME is empty".to_string());
    }

    Ok(PathBuf::from(home)
        .join(".local")
        .join("share")
        .join(APP_DIRECTORY))
}

fn validate_image_reference(relative_path: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative_path);

    if path.is_absolute() {
        return Err(format!("absolute image path rejected: {relative_path}"));
    }

    let mut components = path.components();

    let directory = components.next();

    let file = components.next();

    if components.next().is_some() {
        return Err(format!("nested image path rejected: {relative_path}"));
    }

    match directory {
        Some(Component::Normal(value)) if value == IMAGE_DIRECTORY => {}

        _ => {
            return Err(format!("invalid image directory: {relative_path}"));
        }
    }

    let Some(Component::Normal(file_name)) = file else {
        return Err(format!("invalid image filename: {relative_path}"));
    };

    let Some(file_name) = file_name.to_str() else {
        return Err("image filename is not valid UTF-8".to_string());
    };

    let Some(item_id) = file_name.strip_suffix(IMAGE_EXTENSION) else {
        return Err(format!("image file is not PNG: {relative_path}"));
    };

    if item_id.is_empty()
        || !item_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(format!("invalid image item id: {relative_path}"));
    }

    Ok(path.to_path_buf())
}

fn read_dimensions(path: &Path) -> Result<(u32, u32), String> {
    let file = File::open(path)
        .map_err(|error| format!("failed opening image {}: {error}", path.display(),))?;

    ImageReader::with_format(BufReader::new(file), ImageFormat::Png)
        .into_dimensions()
        .map_err(|error| {
            format!(
                "failed reading image dimensions {}: {error}",
                path.display(),
            )
        })
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err(format!("invalid image dimensions: {width}x{height}"));
    }

    if width > MAX_SOURCE_DIMENSION || height > MAX_SOURCE_DIMENSION {
        return Err(format!(
            "image dimensions exceed UI limit: {width}x{height}"
        ));
    }

    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| "image pixel count overflow".to_string())?;

    if pixels > MAX_SOURCE_PIXELS {
        return Err(format!("image pixel count exceeds UI limit: {pixels}"));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{validate_dimensions, validate_image_reference};

    #[test]
    fn accepts_application_owned_image_reference() {
        assert_eq!(
            validate_image_reference("images/550e8400-e29b-41d4-a716-446655440000.png",)
                .expect("valid reference rejected",),
            PathBuf::from("images/550e8400-e29b-41d4-a716-446655440000.png",),
        );
    }

    #[test]
    fn rejects_absolute_image_reference() {
        assert!(validate_image_reference("/tmp/image.png",).is_err());
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(validate_image_reference("images/../image.png",).is_err());
    }

    #[test]
    fn rejects_nested_reference() {
        assert!(validate_image_reference("images/subfolder/image.png",).is_err());
    }

    #[test]
    fn rejects_non_png_reference() {
        assert!(
            validate_image_reference("images/550e8400-e29b-41d4-a716-446655440000.jpg",).is_err()
        );
    }

    #[test]
    fn accepts_normal_8k_dimensions() {
        assert!(validate_dimensions(7_680, 4_320,).is_ok());
    }

    #[test]
    fn rejects_excessive_pixel_count() {
        assert!(validate_dimensions(8_000, 8_000,).is_err());
    }
}
