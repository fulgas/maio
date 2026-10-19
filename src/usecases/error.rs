//! A use case fails because a rule said no, or because a port broke.

use std::fmt;

#[derive(Debug)]
pub enum Error {
    Rule(crate::domain::Error),
    Port(anyhow::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn rule(&self) -> Option<&crate::domain::Error> {
        match self {
            Self::Rule(e) => Some(e),
            Self::Port(_) => None,
        }
    }
}

impl From<crate::domain::Error> for Error {
    fn from(e: crate::domain::Error) -> Self {
        Self::Rule(e)
    }
}

impl From<anyhow::Error> for Error {
    fn from(e: anyhow::Error) -> Self {
        Self::Port(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rule(e) => e.fmt(f),
            Self::Port(e) => write!(f, "{e:#}"),
        }
    }
}

impl std::error::Error for Error {}
