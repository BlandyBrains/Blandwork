use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::async_trait;
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use tokio::task;
use tokio_postgres::{Row, Statement};

use crate::{BlandworkError, ConnectionPool};


/* 
Ideally this module is not coupled to the spins schema.
*/

#[derive(Clone, Serialize, Deserialize)]
pub struct User {
    id: i32,
    pub email: String,
    pub handle: String,
    pub status: String,
    pub phone: Option<String>,
    password: String,
}

impl From<Row> for User {
    fn from(value: Row) -> Self {
        Self {
            id: value.get(0),
            status: value.get(1),
            handle: value.get(2),
            email: value.get(3),
            phone: value.get(4),
            password: value.get(5),
        }
    }
}

// Here we've implemented `Debug` manually to avoid accidentally logging the
// password hash.
impl std::fmt::Debug for User {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("handle", &self.handle)
            .field("email", &self.email)
            .field("password", &"[redacted]")
            .finish()
    }
}

impl AuthUser for User {
    type Id = i32;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password.as_bytes()
    }
}

#[derive(Deserialize)]
pub struct Credentials {
    pub email: String,
    pub password: String
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("User")
            .field("email", &self.email)
            .field("password", &"[redacted]")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct Vault {
    db: ConnectionPool
}

impl Vault {
    pub fn new(db: ConnectionPool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl AuthnBackend for Vault {
    type User = User;
    type Credentials = Credentials;
    type Error = BlandworkError;

    async fn authenticate(
        &self,
        credentials: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let conn = self.db.get().await?;

        let statement: Statement = conn.prepare(r#"
            select 
                c.id,
                s.name as status,
                c.handle,
                c.email,
                c.phone,
                c.password_hash
            from spins.customer c
            join spins.status s on c.status_id = s.id
            where c.email = $1
        "#).await?;
        
        let row: Option<Row> = conn.query_opt(&statement, &[
                &credentials.email
            ]).await?;

        match row {
            Some(r) => {
                let user: User = User::from(r);

                task::spawn_blocking(move || {
                    let parsed_hash: PasswordHash = PasswordHash::new(&user.password)
                        .map_err(|e| BlandworkError::AuthenticationFailure(e.to_string()))?;
        
                    if Argon2::default().verify_password(&credentials.password.as_bytes(), &parsed_hash).is_ok() {
                        return Ok(Some(user));
                    }
                    Ok(None)
                })
                .await?
            },
            None => {
                Ok(None)
            }
        }
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let conn = self.db.get().await?;

        let statement: Statement = conn.prepare(r#"
            select 
                c.id,
                s.name as status,
                c.handle,
                c.email,
                c.phone,
                c.password_hash
            from spins.customer c
            join spins.status s on c.status_id = s.id
            where c.id = $1
        "#).await?;
        
        let row = conn.query_opt(&statement, &[&user_id]).await?;
        match row {
            Some(r) => {
                Ok(Some(User::from(r)))
            },
            _ => {
                Ok(None)
            }
        }
    }
}

pub type AuthSession = axum_login::AuthSession<Vault>;