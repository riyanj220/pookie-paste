use sqlx::SqlitePool;
use storage::Database;

#[tokio::test]
async fn creates_clipboard_items_schema_on_fresh_database() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let columns = sqlx::query_scalar::<_, String>(
        "
        SELECT name
        FROM pragma_table_info('clipboard_items');
        ",
    )
    .fetch_all(database.pool())
    .await
    .expect("failed reading schema");

    assert!(columns.contains(&"text_content".to_string()));
    assert!(columns.contains(&"file_path".to_string()));
    assert!(columns.contains(&"pinned_at".to_string()));

    let version = sqlx::query_scalar::<_, i64>("PRAGMA user_version;")
        .fetch_one(database.pool())
        .await
        .expect("failed reading user_version");

    assert_eq!(version, 1);
}

#[tokio::test]
async fn upgrades_legacy_phase_10_database_preserving_data() {
    // 1. Create a raw in-memory pool simulating a Phase 10 database before migrations
    let pool = SqlitePool::connect("sqlite::memory:")
        .await
        .expect("failed creating raw sqlite pool");

    // Phase 10 schema (version 0, no pinned_at column)
    sqlx::query(
        "
        CREATE TABLE clipboard_items (
            id TEXT PRIMARY KEY,
            content_type TEXT NOT NULL,
            text_content TEXT,
            file_path TEXT,
            content_hash TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        ",
    )
    .execute(&pool)
    .await
    .expect("failed creating legacy table");

    // Insert legacy text and image records
    sqlx::query(
        "
        INSERT INTO clipboard_items (id, content_type, text_content, file_path, content_hash, created_at)
        VALUES ('text-1', 'text', 'Legacy text snippet', NULL, 'hash-text-1', '2026-08-01T12:00:00Z');
        ",
    )
    .execute(&pool)
    .await
    .expect("failed inserting legacy text item");

    sqlx::query(
        "
        INSERT INTO clipboard_items (id, content_type, text_content, file_path, content_hash, created_at)
        VALUES ('image-1', 'image', NULL, 'images/test.png', 'hash-image-1', '2026-08-01T12:05:00Z');
        ",
    )
    .execute(&pool)
    .await
    .expect("failed inserting legacy image item");

    // Verify initial version is 0
    let initial_version = sqlx::query_scalar::<_, i64>("PRAGMA user_version;")
        .fetch_one(&pool)
        .await
        .expect("failed reading initial version");
    assert_eq!(initial_version, 0);

    // 2. Run migrations
    storage::run_migrations(&pool)
        .await
        .expect("migration failed on legacy database");

    // 3. Verify user_version is upgraded to 1
    let upgraded_version = sqlx::query_scalar::<_, i64>("PRAGMA user_version;")
        .fetch_one(&pool)
        .await
        .expect("failed reading upgraded version");
    assert_eq!(upgraded_version, 1);

    // 4. Verify pinned_at column now exists
    let columns = sqlx::query_scalar::<_, String>(
        "
        SELECT name
        FROM pragma_table_info('clipboard_items');
        ",
    )
    .fetch_all(&pool)
    .await
    .expect("failed reading schema");
    assert!(columns.contains(&"pinned_at".to_string()));

    // 5. Verify existing legacy data survived and pinned_at is NULL
    let rows = sqlx::query_as::<_, storage::StoredClipboardItem>(
        "
        SELECT id, content_type, text_content, file_path, content_hash, created_at, pinned_at
        FROM clipboard_items
        ORDER BY created_at ASC;
        ",
    )
    .fetch_all(&pool)
    .await
    .expect("failed fetching migrated items");

    assert_eq!(rows.len(), 2);

    assert_eq!(rows[0].id, "text-1");
    assert_eq!(rows[0].text_content.as_deref(), Some("Legacy text snippet"));
    assert_eq!(rows[0].pinned_at, None);

    assert_eq!(rows[1].id, "image-1");
    assert_eq!(rows[1].file_path.as_deref(), Some("images/test.png"));
    assert_eq!(rows[1].pinned_at, None);
}
