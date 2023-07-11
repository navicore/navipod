use sqlx::sqlite::SqlitePool;
use std::fs::File;
use std::path::Path;
use tracing::info;

/// # Errors
///
/// Will return `Err` if function cannot create db file
pub async fn init(db_location: String) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    let db_url = format!("sqlite:{db_location}");
    let db_path = Path::new(&db_location);
    if db_path.exists() {
        info!("adding to db {}", db_url);
    } else {
        info!("creating db {}", db_url);
        File::create(&db_location)?;
    }

    let pool = SqlitePool::connect(&db_url).await?;
    Ok(pool)
}

/// # Errors
///
/// Will return `Err` if function cannot create db table
pub async fn create_table(pool: &SqlitePool) -> Result<(), Box<dyn std::error::Error>> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS triples (
            id INTEGER PRIMARY KEY,
            subject TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_subject ON triples (subject);
        CREATE INDEX IF NOT EXISTS idx_predicate ON triples (predicate);
        CREATE INDEX IF NOT EXISTS idx_object ON triples (object);
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Connection, SqliteConnection};
    use std::fs;
    use tokio::runtime::Runtime;

    #[test]
    fn test_init_db() {
        let db_location = "/tmp/test_init_k8p.db";

        // Ensure there's no db file before the test
        let _ = fs::remove_file(db_location);

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let _pool = init(db_location.to_string()).await.unwrap();

            // Check if the database has been created successfully
            let mut conn = SqliteConnection::connect(&format!("sqlite:{}", db_location))
                .await
                .unwrap();
            assert!(conn.ping().await.is_ok());
        });

        // Clean up after the test
        let _ = fs::remove_file(db_location);
    }

    #[test]
    fn test_create_table() {
        let db_location = "/tmp/test_k8p.db";

        // Ensure there's no db file before the test
        let _ = fs::remove_file(db_location);

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let pool = init(db_location.to_string()).await.unwrap();

            match create_table(&pool).await {
                Ok(_) => (),
                Err(e) => panic!("create_table failed with {:?}", e),
            }

            // Check if the table has been created
            let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM triples")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(row.0, 0);
        });

        // Clean up after the test
        let _ = fs::remove_file(db_location);
    }
}
