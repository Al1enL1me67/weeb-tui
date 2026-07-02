use thiserror::Error;

#[derive(Error, Debug)]
pub enum AniTuiError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Manga provider error: {0}")]
    MangaProvider(String),

    #[error("Manga chapter not found: {0}")]
    MangaChapterNotFound(String),

    #[error("Manga image decode failed: {0}")]
    MangaImageDecode(String),

    #[error("Manga HTML parse error: {0}")]
    MangaHtmlParse(String),

    #[error("Player error: {0}")]
    Player(String),

    #[error("UI error: {0}")]
    Ui(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Update error: {0}")]
    Update(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, AniTuiError>;
