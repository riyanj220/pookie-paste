use sqlx::SqlitePool;

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS clipboard_items (

            id TEXT PRIMARY KEY,

            content_type TEXT NOT NULL,

            text_content TEXT,

            file_path TEXT,

            content_hash TEXT NOT NULL,

            created_at TEXT NOT NULL

        );
        ",
    )
    .execute(pool)
    .await?;

    Ok(())
}
