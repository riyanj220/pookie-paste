use sqlx::SqlitePool;

const CURRENT_SCHEMA_VERSION: i64 = 1;

pub async fn run_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    let mut version = get_schema_version(pool).await?;

    while version < CURRENT_SCHEMA_VERSION {
        match version {
            0 => {
                migrate_to_v1(pool).await?;
                set_schema_version(pool, 1).await?;
                version = 1;
            }

            unsupported => {
                return Err(sqlx::Error::Configuration(
                    format!("unsupported database schema version: {unsupported}").into(),
                ));
            }
        }
    }

    Ok(())
}

async fn get_schema_version(pool: &SqlitePool) -> Result<i64, sqlx::Error> {
    let version = sqlx::query_scalar::<_, i64>("PRAGMA user_version;")
        .fetch_one(pool)
        .await?;

    Ok(version)
}

async fn set_schema_version(pool: &SqlitePool, version: i64) -> Result<(), sqlx::Error> {
    let query = format!("PRAGMA user_version = {version};");

    sqlx::query(&query).execute(pool).await?;

    Ok(())
}

async fn migrate_to_v1(pool: &SqlitePool) -> Result<(), sqlx::Error> {
    /*
     * 1. Ensure the base clipboard_items table exists
     * for fresh databases.
     */
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

    /*
     * 2. Inspect existing columns to support upgrade from
     * pre-v1 databases without failing if the column already exists.
     */
    let columns = sqlx::query_scalar::<_, String>(
        "
        SELECT name
        FROM pragma_table_info('clipboard_items');
        ",
    )
    .fetch_all(pool)
    .await?;

    if !columns.iter().any(|name| name == "pinned_at") {
        sqlx::query(
            "
            ALTER TABLE clipboard_items
            ADD COLUMN pinned_at TEXT;
            ",
        )
        .execute(pool)
        .await?;
    }

    Ok(())
}
