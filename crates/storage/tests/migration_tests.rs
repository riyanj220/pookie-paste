use storage::Database;

#[tokio::test]
async fn creates_clipboard_items_table() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let table = sqlx::query_scalar::<_, String>(
        "
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
            AND name = 'clipboard_items'
            ",
    )
    .fetch_one(database.pool())
    .await
    .expect("clipboard_items table should exist");

    assert_eq!(table, "clipboard_items");
}
