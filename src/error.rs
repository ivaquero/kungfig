use thiserror::Error;

#[derive(Debug, Error)]
pub enum KungfigError {
    #[error("item `{0}` does not define a target for the current platform")]
    MissingPlatformTarget(String),
    #[error("unsupported path variable `{0}`")]
    UnknownPathVariable(String),
    #[error("diff only supports regular files: {0}")]
    UnsupportedDiff(String),
}
