//! Hardware- and runtime-independent data and layout contracts.

mod clock;
mod dataset;
mod layout;

pub use clock::*;
pub use dataset::{Dataset, Field, FieldKind};
pub use layout::*;

pub type Result<T> = std::result::Result<T, ValidationError>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError {
    pub path: String,
    pub message: String,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path, self.message)
    }
}

impl std::error::Error for ValidationError {}

pub(crate) fn require(condition: bool, path: impl Into<String>, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ValidationError {
            path: path.into(),
            message: message.into(),
        })
    }
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}
