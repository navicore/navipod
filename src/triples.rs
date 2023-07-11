use sqlx::sqlite::SqlitePool;
use std::error::Error;
use tracing::debug;
use uuid::Uuid;

pub async fn persist_triples(
    triples: Vec<Vec<(String, String, String)>>,
    pool: &SqlitePool,
) -> Result<(), Box<dyn Error>> {
    debug!("persisting {} metrics", triples.len());

    for vec in triples {
        for (subject, predicate, object) in vec {
            sqlx::query(
                r#"
                INSERT INTO triples (subject, predicate, object)
                VALUES (?, ?, ?)
                "#,
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

pub fn format_triples(tuples: Vec<Vec<(String, String)>>) -> Vec<Vec<(String, String, String)>> {
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
fn test_format_triples() {
    let input = vec![vec![
        (
            "k8p_metric_name".to_string(),
            "http_requests_total".to_string(),
        ),
        ("k8p_value".to_string(), "1027".to_string()),
        ("method".to_string(), "post".to_string()),
        ("code".to_string(), "200".to_string()),
        (
            "k8p_description".to_string(),
            "The total number of HTTP requests.".to_string(),
        ),
        ("k8p_type".to_string(), "counter".to_string()),
    ]];

    let output = format_triples(input);
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].len(), 6);
}
