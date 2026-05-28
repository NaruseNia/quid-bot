use sqlx::SqlitePool;
use std::path::Path;

pub async fn init(db_path: &str) -> crate::error::Result<SqlitePool> {
    let dir = Path::new(db_path).parent().unwrap_or(Path::new("."));
    tokio::fs::create_dir_all(dir).await?;

    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", db_path)).await?;

    sqlx::query(include_str!("../migrations/001_init.sql"))
        .execute(&pool)
        .await?;

    Ok(pool)
}
