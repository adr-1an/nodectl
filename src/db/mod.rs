use std::error::Error;
use crate::config::SharedConfig;

pub async fn init_db(cfg: &SharedConfig) -> Result<sqlx::PgPool, Box<dyn Error>> {
    let dsn = format!(
        "postgres://{}:{}@{}:{}/{}",
        cfg.database.user,
        cfg.database.password,
        cfg.database.host,
        cfg.database.port,
        cfg.database.database
    );

    let pool = sqlx::PgPool::connect(&dsn).await?;

    // Test ping
    sqlx::query("SELECT 1 AS one")
        .fetch_one(&pool)
        .await?;
    Ok(pool)
}