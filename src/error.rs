use thiserror::Error;

#[derive(Error, Debug)]
pub enum RaveslingerError {
    #[error("config error: {0}")]
    Config(String),
}
pub type Result<T> = std::result::Result<T, RaveslingerError>;
