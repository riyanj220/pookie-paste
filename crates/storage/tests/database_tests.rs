use storage::Database;

#[tokio::test]
async fn creates_database_connection() {
    let database = Database::new("sqlite::memory:").await;

    assert!(database.is_ok());
}

#[tokio::test]
async fn creates_clipboard_table() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database should initialize");

    let result = sqlx::query(
        "
            SELECT name
            FROM sqlite_master
            WHERE type='table'
            AND name='clipboard_items'
            ",
    )
    .fetch_one(database.pool())
    .await;

    assert!(result.is_ok());
}
