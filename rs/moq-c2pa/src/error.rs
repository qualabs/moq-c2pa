#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to spawn c2patool: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("c2patool I/O error: {0}")]
    Io(#[source] std::io::Error),
    #[error("c2patool exited with {0}")]
    ExitStatus(std::process::ExitStatus),
    #[error("invalid path (contains non-UTF-8 characters)")]
    InvalidPath,
}
