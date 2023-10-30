//!A module manages sqlite via sqlx.
//!
//!The DB has a table for triples with
//!subject,predicate,object cols to enable
//!open-ended scheema-less variable len record types.
//!
use sqlx::Pool;
use sqlx::Row;
use sqlx::Sqlite;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use tracing::debug;

const BASE_URI: &str = "http://k8p.navicore.tech";
const RESOURCE_PREFIX: &str = "res";
const PROPERTY_PREFIX: &str = "prop";

/// # Errors
///
/// Will return `Err` if function cannot create db file
pub async fn init(db_location: String) -> Result<Pool<Sqlite>, Box<dyn std::error::Error>> {
    let db_url = format!("sqlite:{db_location}");
    let db_path = Path::new(&db_location);
    if db_path.exists() {
        debug!("adding to db {}", db_url);
    } else {
        debug!("creating db {}", db_url);
        File::create(&db_location)?;
    }

    let pool = Pool::connect(&db_url).await?;
    Ok(pool)
}

/// # Errors
///
/// Will return `Err` if function cannot create db table
pub async fn create_table(pool: &Pool<Sqlite>) -> Result<(), Box<dyn std::error::Error>> {
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

/// # Errors
///
/// Will return `Err` if function cannot read db file
pub async fn export_to_nt_rdf(
    pool: &Pool<Sqlite>,
    rdffile_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(rdffile_name)?;

    let rows = sqlx::query("SELECT subject, predicate, object FROM triples")
        .fetch_all(pool)
        .await?;

    for row in rows {
        let subject: String = row.get("subject");
        let predicate: String = row.get("predicate");
        let object: String = row.get("object");

        let subject_uri = format!("{BASE_URI}/resource/{subject}");
        let predicate_uri = format!("{BASE_URI}/property/{predicate}");

        //let object = object.replace("\"", "\\\"");
        let object = object.replace('\"', "\\\"");

        writeln!(file, "<{subject_uri}> <{predicate_uri}> \"{object}\" .")?;
    }

    Ok(())
}

/// # Errors
///
/// Will return `Err` if function cannot create db table
pub async fn report(pool: &sqlx::SqlitePool) -> Result<String, sqlx::Error> {
    let total_records: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM triples")
        .fetch_one(pool)
        .await?;

    let unique_subjects: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT subject) FROM triples")
        .fetch_one(pool)
        .await?;

    let unique_predicates: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT predicate) FROM triples")
        .fetch_one(pool)
        .await?;

    let unique_objects: (i64,) = sqlx::query_as("SELECT COUNT(DISTINCT object) FROM triples")
        .fetch_one(pool)
        .await?;

    Ok(format!(
        "Records: {}\nSubjects: {}\nPredicates: {}\nObjects: {}",
        total_records.0, unique_subjects.0, unique_predicates.0, unique_objects.0
    ))
}

/// # Errors
///
/// Will return `Err` if function cannot read db file
pub async fn export_to_ttl_rdf(
    pool: &Pool<Sqlite>,
    ttlfile_name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = File::create(ttlfile_name)?;

    // Define the prefixes
    writeln!(
        file,
        "@prefix {RESOURCE_PREFIX}: <{BASE_URI}/resource/> .\n"
    )?;
    writeln!(
        file,
        "@prefix {PROPERTY_PREFIX}: <{BASE_URI}/property/> .\n"
    )?;

    let rows = sqlx::query("SELECT subject, predicate, object FROM triples")
        .fetch_all(pool)
        .await?;

    let mut triples: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();

    for row in rows {
        let subject: String = row.get("subject");
        let predicate: String = row.get("predicate");
        let object: String = row.get("object");

        // Use the prefixes instead of the full URIs
        let subject_uri = format!("{RESOURCE_PREFIX}:{subject}");
        let predicate_uri = format!("{PROPERTY_PREFIX}:{predicate}");

        let object = object.replace('\"', "\\\"");

        triples
            .entry(subject_uri)
            .or_insert_with(HashMap::new)
            .entry(predicate_uri)
            .or_insert_with(Vec::new)
            .push(object);
    }

    for (subject, predicates) in triples {
        writeln!(file, "{subject} ")?;
        let pred_vec: Vec<String> = predicates
            .iter()
            .map(|(predicate, objects)| {
                let obj_str = objects
                    .iter()
                    .map(|obj| format!("\"{obj}\""))
                    .collect::<Vec<_>>()
                    .join(" , ");
                format!("    {predicate} {obj_str} ;")
            })
            .collect();
        let predicates_str = pred_vec.join("\n");
        writeln!(file, "{predicates_str} .\n")?; // Add an empty line between records
    }

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
