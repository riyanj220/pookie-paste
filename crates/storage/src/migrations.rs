use sqlx::SqlitePool;

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "
        CREATE TABLE IF NOT EXISTS clipboard_items (

            id TEXT PRIMARY KEY,

            content TEXT NOT NULL,

            content_hash TEXT NOT NULL,

            content_type TEXT NOT NULL,

            created_at TEXT NOT NULL

        );
        ",
    )
    .execute(pool)
    .await?;

    Ok(())
}
