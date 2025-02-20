use anyhow::Result;
use sqlx::{SqlitePool, migrate, query};
use tracing::debug;

pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(database_url).await?;
        migrate!().run(&pool).await?;
        Ok(Self { pool })
    }

    pub async fn add_posted_url(&self, url: &str) -> Result<()> {
        debug!("Storing {url} in posted_urls");
        query!("INSERT INTO posted_urls (url) VALUES (?)", url)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn remove_old_stored_posts(&self) -> Result<()> {
        debug!("Removing old posted_urls entries");
        query!("DELETE FROM posted_urls WHERE ROWID IN (SELECT ROWID FROM posted_urls ORDER BY ROWID DESC LIMIT -1 OFFSET 25000)").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn has_posted_url(&self, url: &str) -> Result<bool> {
        debug!("Checking if {url} exists in posted_urls table");
        Ok(query!("SELECT url FROM posted_urls WHERE url = ?", url)
            .fetch_optional(&self.pool)
            .await?
            .is_some())
    }
}
