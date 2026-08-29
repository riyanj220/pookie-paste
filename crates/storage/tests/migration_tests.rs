use storage::Database;

#[tokio::test]
async fn creates_clipboard_items_schema() {
    let database = Database::new("sqlite::memory:")
        .await
        .expect("database initialization failed");

    let columns = sqlx::query_scalar::<_, String>(
        "
            SELECT name
            FROM pragma_table_info(
                'clipboard_items'
            )
            ",
    )
    .fetch_all(database.pool())
    .await
    .expect("failed reading schema");

    assert!(columns.contains(&"text_content".to_string()));

    assert!(columns.contains(&"file_path".to_string()));
}
