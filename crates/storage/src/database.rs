use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;

use std::str::FromStr;

use crate::migrations::run_migrations;

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::from_str(database_url)?.create_if_missing(true),
        )
        .await?;

        run_migrations(&pool).await?;

        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
