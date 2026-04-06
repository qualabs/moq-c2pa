#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to spawn signer: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("signer I/O error: {0}")]
    Io(#[source] std::io::Error),
    #[error("signer exited with {0}")]
    ExitStatus(std::process::ExitStatus),
    #[error("invalid path (contains non-UTF-8 characters)")]
    InvalidPath,
    #[error("failed to parse manifest JSON: {0}")]
    ManifestParse(#[source] serde_json::Error),
    #[error("manifest missing required field: {0}")]
    ManifestMissingField(&'static str),
}
