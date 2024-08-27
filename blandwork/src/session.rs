use async_trait::async_trait;
use tower_sessions::{
    cookie::time::OffsetDateTime, session::{self, Id, Record}, session_store::{self, Result}, ExpiredDeletion, SessionStore
};
use crate::ConnectionPool;


#[derive(thiserror::Error, Debug)]
pub enum PostgresSessionError {
    /// A variant to map session errors.
    #[error(transparent)]
    Session(#[from] session::Error),

    #[error("{0}")]
    Database(#[from] tokio_postgres::Error),

    #[error("{0}")]
    Connection(#[from] bb8::RunError<tokio_postgres::Error>),

    /// A variant to map `serde_json` errors.
    #[error("JSON serialization/deserialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// A variant to map `rmp_serde` encode errors.
    #[error("Rust MsgPack encode error: {0}")]
    RmpSerdeEncode(#[from] rmp_serde::encode::Error),

    /// A variant to map `rmp_serde` decode errors.
    #[error("Rust MsgPack decode error: {0}")]
    RmpSerdeDecode(#[from] rmp_serde::decode::Error),
}

// pub type SessionError = Box<dyn std::error::Error>;

#[derive(Debug, Clone)]
pub struct BlandworkSessionStore {
    pool: ConnectionPool,
    schema_name: String,
    table_name: String,
}

impl BlandworkSessionStore {
    pub fn new(pool: &ConnectionPool, schema: String, session_table: String) -> Self {
        Self {
            pool: pool.clone(),
            schema_name: schema,
            table_name: session_table,
        }
    }
}

#[async_trait]
impl ExpiredDeletion for BlandworkSessionStore {
    async fn delete_expired(&self) -> Result<()> {
        let query = format!(
            r#"
            delete from "{schema_name}"."{table_name}"
            where expiry_date < (now() at time zone 'utc')
            "#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        let _ = conn.execute(&query, &[])
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        Ok(())
    }
}

#[async_trait]
impl SessionStore for BlandworkSessionStore {

    // async fn create(&self, session_record: &mut Record) -> Result<()> {
    //     TODO - improved ID generation
    // }

    /// Saves the provided session record to the store.
    ///
    /// This method is intended for updating the state of an existing session.
    async fn save(&self, record: &Record) -> Result<()> {
        let query = format!(
            r#"
            insert into "{schema_name}"."{table_name}" (id, data, expiry_date)
            values ($1, $2, $3)
            on conflict (id) do update
            set
              data = excluded.data,
              expiry_date = excluded.expiry_date
            "#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        let data = rmp_serde::to_vec(&record)
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        
        let _ = conn
            .execute(
                &query,
                &[&record.id.to_string(), &data, &record.expiry_date],
            )
            .await;
        Ok(())
    }

    /// Loads an existing session record from the store using the provided ID.
    ///
    /// If a session with the given ID exists, it is returned. If the session
    /// does not exist or has been invalidated (e.g., expired), `None` is
    /// returned.
    async fn load(&self, session_id: &Id) -> Result<Option<Record>> {
        let query = format!(
            r#"
            select data from "{schema_name}"."{table_name}"
            where id = $1 and expiry_date > $2
            "#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        let cur_date = OffsetDateTime::now_utc();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;
        
        let rows = conn
            .query(&query, &[&session_id.to_string(), &cur_date])
            .await
            .map_err(|e| session_store::Error::Backend(e.to_string()))?;

        if let Some(row) = rows.get(0) {
            let data: Vec<u8> = row.get("data");
            let record: Record = rmp_serde::from_slice(&data)
                .map_err(|e| session_store::Error::Backend(e.to_string()))?;

            return Ok(Some(record));
        }

        Ok(None)
    }

    /// Deletes a session record from the store using the provided ID.
    ///
    /// If the session exists, it is removed from the store.
    async fn delete(&self, session_id: &Id) -> Result<()> {
        let query = format!(
            r#"delete from "{schema_name}"."{table_name}" where id = $1"#,
            schema_name = self.schema_name,
            table_name = self.table_name
        );
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e|session_store::Error::Backend(e.to_string()))?;
        let _ = conn.execute(&query, &[&session_id.to_string()]).await;
        Ok(())
    }
}