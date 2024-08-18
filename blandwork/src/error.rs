use std::fmt;
use axum::response::{IntoResponse, Response};
use hyper::StatusCode;
use thiserror::Error;


#[derive(Debug)]
pub struct ValidationError {
    errors: Vec<String>,
}

impl ValidationError {
    pub fn new() -> ValidationError {
        ValidationError {
            errors: Vec::new()
        }
    }

    pub fn add(&mut self, message: &str) {
        self.errors.push(message.to_owned());
    }

    pub fn count(&self) -> usize {
        return self.errors.len();
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for err in self.errors.iter() {
            write!(f, "{}\n", err)?
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}


#[derive(Error, Debug)]
pub enum BlandworkError {
    #[error("validation errors {0}")]
    Validation(#[from] ValidationError),

    #[error("{0}")]
    Database(#[from] tokio_postgres::Error),

    #[error("{0}")]
    Connection(#[from] bb8::RunError<tokio_postgres::Error>)
}

impl IntoResponse for BlandworkError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            BlandworkError::Validation(e) => {
                tracing::warn!("{}", e);
                (StatusCode::BAD_REQUEST, format!("{:#?}", e))
            },
            ee => {
                tracing::error!("{}", ee);
                (StatusCode::INTERNAL_SERVER_ERROR, "bad thing happened".to_owned())
            }
        };

        Response::builder()
            .status(status)
            // .header("Content-Type", "application/json")
            .body(message.into())
            .unwrap()
    }
}