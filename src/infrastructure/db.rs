use std::str::FromStr;

use sqlx::{
    Pool, Sqlite,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
};

pub type DbPool = Pool<Sqlite>;

pub async fn init_db(database_url: &str) -> Result<DbPool, sqlx::Error> {
    let options = SqliteConnectOptions::from_str(database_url)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS elections (
            id TEXT PRIMARY KEY NOT NULL,
            manifest TEXT NOT NULL CHECK (json_valid(manifest)),
            status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
            outcome TEXT CHECK (outcome IS NULL OR json_valid(outcome)),
            created_at TEXT NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS election_events (
            election_id TEXT NOT NULL,
            sequence INTEGER NOT NULL CHECK (sequence > 0),
            event_type TEXT NOT NULL CHECK (event_type = 'vote_cast'),
            payload TEXT NOT NULL CHECK (json_valid(payload)),
            prev_hash TEXT NOT NULL CHECK (length(prev_hash) = 64),
            current_hash TEXT NOT NULL CHECK (length(current_hash) = 64),
            timestamp TEXT NOT NULL,
            PRIMARY KEY (election_id, sequence),
            FOREIGN KEY (election_id) REFERENCES elections(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(&pool)
    .await?;

    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn initializes_a_database_with_constrained_persistence_schema() {
        let pool = init_db("sqlite::memory:").await.unwrap();

        let election_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('elections')")
                .fetch_all(&pool)
                .await
                .unwrap();
        let event_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('election_events')")
                .fetch_all(&pool)
                .await
                .unwrap();

        assert!(election_columns.contains(&"manifest".to_owned()));
        assert!(election_columns.contains(&"status".to_owned()));
        assert!(election_columns.contains(&"outcome".to_owned()));
        assert!(event_columns.contains(&"sequence".to_owned()));
        assert!(event_columns.contains(&"prev_hash".to_owned()));
        assert!(event_columns.contains(&"current_hash".to_owned()));
    }
}
