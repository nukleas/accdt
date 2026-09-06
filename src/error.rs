#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("invalid XML in {part}: {source}")]
    Xml {
        part: String,
        #[source]
        source: roxmltree::Error,
    },
    #[error("package has no part {0}")]
    MissingPart(String),
    #[error("no {kind} named {name}")]
    MissingObject { kind: &'static str, name: String },
    #[error("{0} is not an Access template package")]
    NotAPackage(String),
    #[error("invalid {part}: {reason}")]
    Invalid { part: String, reason: String },
}

pub type Result<T> = std::result::Result<T, Error>;
