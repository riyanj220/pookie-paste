use std::collections::HashSet;
use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const IMAGE_DIRECTORY: &str = "images";

const IMAGE_EXTENSION: &str = ".png";

const TEMP_FILE_PREFIX: &str = ".pookie-image-";

const TEMP_FILE_SUFFIX: &str = ".tmp";

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum ImageStoreError {
    EmptyImage,

    InvalidItemId(String),

    InvalidRelativePath(String),

    Io {
        operation: &'static str,
        source: io::Error,
    },
}

impl fmt::Display for ImageStoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage => {
                write!(formatter, "image payload is empty")
            }

            Self::InvalidItemId(item_id) => {
                write!(formatter, "invalid image item id: {item_id}")
            }

            Self::InvalidRelativePath(path) => {
                write!(formatter, "invalid image relative path: {path}")
            }

            Self::Io { operation, source } => {
                write!(formatter, "{operation}: {source}")
            }
        }
    }
}

impl std::error::Error for ImageStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),

            _ => None,
        }
    }
}

/// Filesystem-backed storage for canonical Pookie image
/// payloads.
///
/// The store owns files beneath:
///
/// ```text
/// <data_directory>/images/
/// ```
///
/// Database-facing paths are intentionally relative:
///
/// ```text
/// images/<item-id>.png
/// ```
///
/// This keeps persisted database rows independent from a
/// user's exact HOME or XDG_DATA_HOME.
#[derive(Debug, Clone)]
pub struct ImageStore {
    data_directory: PathBuf,
}

impl ImageStore {
    pub fn new(data_directory: impl Into<PathBuf>) -> Self {
        Self {
            data_directory: data_directory.into(),
        }
    }

    pub fn data_directory(&self) -> &Path {
        &self.data_directory
    }

    pub fn images_directory(&self) -> PathBuf {
        self.data_directory.join(IMAGE_DIRECTORY)
    }

    /// Generate the relative application-owned image path
    /// associated with one history item.
    ///
    /// Expected result:
    ///
    /// ```text
    /// images/<uuid>.png
    /// ```
    pub fn relative_path_for(&self, item_id: &str) -> Result<PathBuf, ImageStoreError> {
        validate_item_id(item_id)?;

        Ok(PathBuf::from(IMAGE_DIRECTORY).join(format!("{item_id}{IMAGE_EXTENSION}")))
    }

    /// Resolve one application-owned relative image path into
    /// its absolute filesystem location.
    ///
    /// Only the exact layout:
    ///
    /// ```text
    /// images/<safe-name>.png
    /// ```
    ///
    /// is accepted.
    ///
    /// Absolute paths, traversal, nested paths, and arbitrary
    /// files are rejected.
    pub fn resolve_relative_path(&self, relative_path: &str) -> Result<PathBuf, ImageStoreError> {
        let path = validate_relative_image_path(relative_path)?;

        Ok(self.data_directory.join(path))
    }

    /// Persist a canonical PNG image.
    ///
    /// The caller is responsible for supplying bytes that
    /// have already passed Pookie's image codec and normal
    /// ClipboardPolicy.
    ///
    /// Writes use a temporary file in the same directory and
    /// then rename it into place.
    pub async fn write_image(
        &self,
        item_id: &str,
        canonical_png: &[u8],
    ) -> Result<String, ImageStoreError> {
        if canonical_png.is_empty() {
            return Err(ImageStoreError::EmptyImage);
        }

        let relative_path = self.relative_path_for(item_id)?;

        let images_directory = self.images_directory();

        tokio::fs::create_dir_all(&images_directory)
            .await
            .map_err(|source| ImageStoreError::Io {
                operation: "failed creating image storage directory",
                source,
            })?;

        let final_path = self.data_directory.join(&relative_path);

        let temporary_path = temporary_path(&images_directory);

        if let Err(source) = tokio::fs::write(&temporary_path, canonical_png).await {
            let _ = tokio::fs::remove_file(&temporary_path).await;

            return Err(ImageStoreError::Io {
                operation: "failed writing temporary image file",
                source,
            });
        }

        if let Err(source) = tokio::fs::rename(&temporary_path, &final_path).await {
            let _ = tokio::fs::remove_file(&temporary_path).await;

            return Err(ImageStoreError::Io {
                operation: "failed committing image file",
                source,
            });
        }

        Ok(path_to_storage_string(&relative_path))
    }

    pub async fn read_image(&self, relative_path: &str) -> Result<Vec<u8>, ImageStoreError> {
        let absolute_path = self.resolve_relative_path(relative_path)?;

        tokio::fs::read(absolute_path)
            .await
            .map_err(|source| ImageStoreError::Io {
                operation: "failed reading image file",
                source,
            })
    }

    /// Delete one application-owned image.
    ///
    /// Returns:
    ///
    /// ```text
    /// true  -> file existed and was removed
    /// false -> file was already absent
    /// ```
    pub async fn delete_image(&self, relative_path: &str) -> Result<bool, ImageStoreError> {
        let absolute_path = self.resolve_relative_path(relative_path)?;

        match tokio::fs::remove_file(absolute_path).await {
            Ok(()) => Ok(true),

            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),

            Err(source) => Err(ImageStoreError::Io {
                operation: "failed deleting image file",
                source,
            }),
        }
    }

    pub async fn image_exists(&self, relative_path: &str) -> Result<bool, ImageStoreError> {
        let absolute_path = self.resolve_relative_path(relative_path)?;

        match tokio::fs::metadata(absolute_path).await {
            Ok(metadata) => Ok(metadata.is_file()),

            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),

            Err(source) => Err(ImageStoreError::Io {
                operation: "failed checking image file",
                source,
            }),
        }
    }

    /// Remove image files that are no longer referenced by
    /// SQLite.
    ///
    /// This also removes temporary files left behind if the
    /// process crashed between temporary-file creation and
    /// final rename.
    ///
    /// Only files owned by Pookie are considered:
    ///
    /// ```text
    /// images/<safe-id>.png
    /// .pookie-image-*.tmp
    /// ```
    pub async fn cleanup_unreferenced(
        &self,
        referenced_paths: &HashSet<String>,
    ) -> Result<usize, ImageStoreError> {
        let images_directory = self.images_directory();

        let mut entries = match tokio::fs::read_dir(&images_directory).await {
            Ok(entries) => entries,

            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(0);
            }

            Err(source) => {
                return Err(ImageStoreError::Io {
                    operation: "failed opening image storage directory",
                    source,
                });
            }
        };

        let mut removed = 0usize;

        while let Some(entry) =
            entries
                .next_entry()
                .await
                .map_err(|source| ImageStoreError::Io {
                    operation: "failed reading image storage directory",
                    source,
                })?
        {
            let file_type = entry
                .file_type()
                .await
                .map_err(|source| ImageStoreError::Io {
                    operation: "failed reading image file type",
                    source,
                })?;

            if !file_type.is_file() {
                continue;
            }

            let file_name = entry.file_name();

            let Some(file_name) = file_name.to_str() else {
                continue;
            };

            let is_temp =
                file_name.starts_with(TEMP_FILE_PREFIX) && file_name.ends_with(TEMP_FILE_SUFFIX);

            if is_temp {
                tokio::fs::remove_file(entry.path())
                    .await
                    .map_err(|source| ImageStoreError::Io {
                        operation: "failed removing stale temporary image file",
                        source,
                    })?;

                removed += 1;

                continue;
            }

            let relative_path = PathBuf::from(IMAGE_DIRECTORY).join(file_name);

            let relative_path_string = path_to_storage_string(&relative_path);

            if validate_relative_image_path(&relative_path_string).is_err() {
                continue;
            }

            if referenced_paths.contains(&relative_path_string) {
                continue;
            }

            tokio::fs::remove_file(entry.path())
                .await
                .map_err(|source| ImageStoreError::Io {
                    operation: "failed removing orphan image file",
                    source,
                })?;

            removed += 1;
        }

        Ok(removed)
    }
}

fn validate_item_id(item_id: &str) -> Result<(), ImageStoreError> {
    if item_id.is_empty()
        || !item_id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-')
    {
        return Err(ImageStoreError::InvalidItemId(item_id.to_string()));
    }

    Ok(())
}

fn validate_relative_image_path(relative_path: &str) -> Result<PathBuf, ImageStoreError> {
    let path = Path::new(relative_path);

    if path.is_absolute() {
        return Err(ImageStoreError::InvalidRelativePath(
            relative_path.to_string(),
        ));
    }

    let mut components = path.components();

    let directory = components.next();

    let file = components.next();

    if components.next().is_some() {
        return Err(ImageStoreError::InvalidRelativePath(
            relative_path.to_string(),
        ));
    }

    match directory {
        Some(Component::Normal(value)) if value == OsStr::new(IMAGE_DIRECTORY) => {}

        _ => {
            return Err(ImageStoreError::InvalidRelativePath(
                relative_path.to_string(),
            ));
        }
    }

    let file_name = match file {
        Some(Component::Normal(value)) => value,

        _ => {
            return Err(ImageStoreError::InvalidRelativePath(
                relative_path.to_string(),
            ));
        }
    };

    let Some(file_name) = file_name.to_str() else {
        return Err(ImageStoreError::InvalidRelativePath(
            relative_path.to_string(),
        ));
    };

    let Some(item_id) = file_name.strip_suffix(IMAGE_EXTENSION) else {
        return Err(ImageStoreError::InvalidRelativePath(
            relative_path.to_string(),
        ));
    };

    validate_item_id(item_id)
        .map_err(|_| ImageStoreError::InvalidRelativePath(relative_path.to_string()))?;

    Ok(path.to_path_buf())
}

fn temporary_path(images_directory: &Path) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);

    let process_id = std::process::id();

    images_directory.join(format!(
        "{TEMP_FILE_PREFIX}{process_id}-{counter}{TEMP_FILE_SUFFIX}"
    ))
}

fn path_to_storage_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{ImageStore, ImageStoreError};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new() -> Self {
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before Unix epoch")
                .as_nanos();

            let counter = TEST_COUNTER.fetch_add(1, Ordering::Relaxed);

            let path = std::env::temp_dir().join(format!(
                "pookie-image-store-test-{}-{timestamp}-{counter}",
                std::process::id(),
            ));

            fs::create_dir_all(&path).expect("failed creating test directory");

            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn generates_relative_uuid_owned_path() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let path = store
            .relative_path_for("550e8400-e29b-41d4-a716-446655440000")
            .expect("relative path generation failed");

        assert_eq!(
            path,
            PathBuf::from("images/550e8400-e29b-41d4-a716-446655440000.png"),
        );
    }

    #[test]
    fn rejects_unsafe_item_ids() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        assert!(matches!(
            store.relative_path_for("../escape"),
            Err(ImageStoreError::InvalidItemId(_))
        ));

        assert!(matches!(
            store.relative_path_for("image/name"),
            Err(ImageStoreError::InvalidItemId(_))
        ));

        assert!(matches!(
            store.relative_path_for(""),
            Err(ImageStoreError::InvalidItemId(_))
        ));
    }

    #[test]
    fn resolves_application_relative_path() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let absolute = store
            .resolve_relative_path("images/550e8400-e29b-41d4-a716-446655440000.png")
            .expect("relative path resolution failed");

        assert_eq!(
            absolute,
            directory
                .path
                .join("images/550e8400-e29b-41d4-a716-446655440000.png"),
        );
    }

    #[test]
    fn rejects_absolute_paths() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        assert!(matches!(
            store.resolve_relative_path("/tmp/image.png"),
            Err(ImageStoreError::InvalidRelativePath(_))
        ));
    }

    #[test]
    fn rejects_parent_traversal() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        assert!(matches!(
            store.resolve_relative_path("images/../outside.png"),
            Err(ImageStoreError::InvalidRelativePath(_))
        ));
    }

    #[test]
    fn rejects_nested_image_paths() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        assert!(matches!(
            store.resolve_relative_path("images/nested/item.png"),
            Err(ImageStoreError::InvalidRelativePath(_))
        ));
    }

    #[tokio::test]
    async fn writes_and_reads_image() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let item_id = "550e8400-e29b-41d4-a716-446655440000";

        let bytes = vec![1, 2, 3, 4, 5];

        let relative_path = store
            .write_image(item_id, &bytes)
            .await
            .expect("image write failed");

        assert_eq!(
            relative_path,
            "images/550e8400-e29b-41d4-a716-446655440000.png",
        );

        let stored = store
            .read_image(&relative_path)
            .await
            .expect("image read failed");

        assert_eq!(stored, bytes);
    }

    #[tokio::test]
    async fn creates_image_directory_lazily() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let image_directory = store.images_directory();

        assert!(!image_directory.exists());

        store
            .write_image("550e8400-e29b-41d4-a716-446655440000", &[1, 2, 3])
            .await
            .expect("image write failed");

        assert!(image_directory.is_dir());
    }

    #[tokio::test]
    async fn rejects_empty_image() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let result = store
            .write_image("550e8400-e29b-41d4-a716-446655440000", &[])
            .await;

        assert!(matches!(result, Err(ImageStoreError::EmptyImage)));
    }

    #[tokio::test]
    async fn reports_image_existence() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let relative_path = store
            .write_image("550e8400-e29b-41d4-a716-446655440000", &[1, 2, 3])
            .await
            .expect("image write failed");

        assert!(
            store
                .image_exists(&relative_path)
                .await
                .expect("existence check failed"),
        );
    }

    #[tokio::test]
    async fn deletes_image() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let relative_path = store
            .write_image("550e8400-e29b-41d4-a716-446655440000", &[1, 2, 3])
            .await
            .expect("image write failed");

        let deleted = store
            .delete_image(&relative_path)
            .await
            .expect("image delete failed");

        assert!(deleted);

        let exists = store
            .image_exists(&relative_path)
            .await
            .expect("existence check failed");

        assert!(!exists);
    }

    #[tokio::test]
    async fn deleting_missing_image_is_idempotent() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let deleted = store
            .delete_image("images/550e8400-e29b-41d4-a716-446655440000.png")
            .await
            .expect("image delete failed");

        assert!(!deleted);
    }

    #[tokio::test]
    async fn successful_write_leaves_no_temp_files() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        store
            .write_image("550e8400-e29b-41d4-a716-446655440000", &[1, 2, 3])
            .await
            .expect("image write failed");

        let entries = fs::read_dir(store.images_directory())
            .expect("failed reading image directory")
            .collect::<Result<Vec<_>, _>>()
            .expect("failed reading directory entry");

        assert_eq!(entries.len(), 1);
    }

    #[tokio::test]
    async fn cleanup_removes_orphan_images() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        let kept = store
            .write_image("550e8400-e29b-41d4-a716-446655440000", &[1])
            .await
            .expect("kept image write failed");

        let orphan = store
            .write_image("550e8400-e29b-41d4-a716-446655440001", &[2])
            .await
            .expect("orphan image write failed");

        let referenced = HashSet::from([kept.clone()]);

        let removed = store
            .cleanup_unreferenced(&referenced)
            .await
            .expect("cleanup failed");

        assert_eq!(removed, 1);

        assert!(
            store
                .image_exists(&kept)
                .await
                .expect("kept image existence check failed"),
        );

        assert!(
            !store
                .image_exists(&orphan)
                .await
                .expect("orphan image existence check failed"),
        );
    }

    #[tokio::test]
    async fn cleanup_removes_stale_temp_files() {
        let directory = TestDirectory::new();

        let store = ImageStore::new(&directory.path);

        fs::create_dir_all(store.images_directory()).expect("failed creating images directory");

        let temp_path = store.images_directory().join(".pookie-image-test.tmp");

        fs::write(&temp_path, [1, 2, 3]).expect("failed writing stale temp file");

        let removed = store
            .cleanup_unreferenced(&HashSet::new())
            .await
            .expect("cleanup failed");

        assert_eq!(removed, 1);

        assert!(!temp_path.exists());
    }
}
