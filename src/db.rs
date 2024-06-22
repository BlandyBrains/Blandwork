use bb8::{Pool, PooledConnection};
use bb8_postgres::PostgresConnectionManager;
use tokio_postgres::NoTls;

pub type Connection<'a> = PooledConnection<'a, PostgresConnectionManager<tokio_postgres::NoTls>>;
pub type ConnectionPool = Pool<PostgresConnectionManager<NoTls>>;