use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Xml { part: String, source: roxmltree::Error },
    MissingPart(String),
    NotAPackage(String),
    Invalid { part: String, reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "I/O error: {e}"),
            Error::Zip(e) => write!(f, "zip error: {e}"),
            Error::Xml { part, source } => write!(f, "invalid XML in {part}: {source}"),
            Error::MissingPart(p) => write!(f, "package has no part {p}"),
            Error::NotAPackage(p) => write!(f, "{p} is not an Access template package"),
            Error::Invalid { part, reason } => write!(f, "invalid {part}: {reason}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io(e) => Some(e),
            Error::Zip(e) => Some(e),
            Error::Xml { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(e: zip::result::ZipError) -> Self {
        Error::Zip(e)
    }
}
