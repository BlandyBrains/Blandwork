use async_trait::async_trait;
use tower_sessions::{session::{Id, Record}, session_store::Result, SessionStore as Store};


// pub type SessionError = Box<dyn std::error::Error>;

#[derive(Debug)]
pub struct SessionStore {

}

#[async_trait]
impl Store for SessionStore {
    async fn create(&self, session_record: &mut Record) -> Result<()> {
        Ok(default_create(self, session_record).await?)
    }

    /// Saves the provided session record to the store.
    ///
    /// This method is intended for updating the state of an existing session.
    async fn save(&self, session_record: &Record) -> Result<()> {
        Ok(())
    }

    /// Loads an existing session record from the store using the provided ID.
    ///
    /// If a session with the given ID exists, it is returned. If the session
    /// does not exist or has been invalidated (e.g., expired), `None` is
    /// returned.
    async fn load(&self, session_id: &Id) -> Result<Option<Record>> {
        Ok(None)
    }

    /// Deletes a session record from the store using the provided ID.
    ///
    /// If the session exists, it is removed from the store.
    async fn delete(&self, session_id: &Id) -> Result<()> {
        Ok(())
    }
}

async fn default_create<S: Store + ?Sized>(
    store: &S,
    session_record: &mut Record,
) -> Result<()> {
    tracing::warn!(
        "The default implementation of `SessionStore::create` is being used, which relies on \
         `SessionStore::save`. To properly handle potential ID collisions, it is recommended that \
         stores implement their own version of `SessionStore::create`."
    );
    store.save(session_record).await?;
    Ok(())
}