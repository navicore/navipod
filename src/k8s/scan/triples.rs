use sqlx::Pool;
use sqlx::Sqlite;
use std::error::Error;
use tracing::debug;
use uuid::Uuid;

/// # Errors
///
/// Will return `Err` if `pool` does not represent a healthy db interface.
pub async fn persist(
    triples: Vec<Vec<(String, String, String)>>,
    pool: &Pool<Sqlite>,
) -> Result<(), Box<dyn Error>> {
    debug!("persisting {} metrics", triples.len());

    for vec in triples {
        for (subject, predicate, object) in vec {
            sqlx::query(
                r"
                INSERT INTO triples (subject, predicate, object)
                VALUES (?, ?, ?)
                ",
            )
            .bind(subject)
            .bind(predicate)
            .bind(object)
            .execute(pool)
            .await?;
        }
    }

    Ok(())
}

/// Each `Vec` is a collection of name value pairs.  This function generates a
/// `uuid` to group the pairs together for when they are later stored with other
/// metrics, making them now a collection of triples.
#[must_use]
pub fn format(tuples: Vec<Vec<(String, String)>>) -> Vec<Vec<(String, String, String)>> {
    tuples
        .into_iter()
        .map(|inner_vec| {
            let my_uuid = Uuid::new_v4().to_string();
            inner_vec
                .into_iter()
                .map(|(first, second)| (my_uuid.clone(), first, second))
                .collect::<Vec<(String, String, String)>>()
        })
        .collect()
}

#[test]
fn test_format() {
    let input = vec![vec![
        (
            "navipod_metric_name".to_string(),
            "http_requests_total".to_string(),
        ),
        ("navipod_value".to_string(), "1027".to_string()),
        ("method".to_string(), "post".to_string()),
        ("code".to_string(), "200".to_string()),
        (
            "navipod_description".to_string(),
            "The total number of HTTP requests.".to_string(),
        ),
        ("navipod_type".to_string(), "counter".to_string()),
    ]];

    let output = format(input);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].len(), 6);
}
