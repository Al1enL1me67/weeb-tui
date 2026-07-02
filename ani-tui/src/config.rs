use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub sources: SourcesConfig,

    #[serde(default)]
    pub prowlarr: Option<ProwlarrConfig>,

    #[serde(default)]
    pub theme: ThemeConfig,

    #[serde(default)]
    pub manga: MangaConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourcesConfig {
    #[serde(default = "default_true")]
    pub allanime: bool,

    #[serde(default = "default_true")]
    pub kkphim: bool,

    #[serde(default = "default_true")]
    pub ophim: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProwlarrConfig {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    pub primary_color: String,
    pub secondary_color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MangaConfig {
    #[serde(default = "default_manga_provider")]
    pub default_provider: String,
    #[serde(default)]
    pub preferred_language: String,
    #[serde(default)]
    pub image_quality: String,
    #[serde(default = "default_preload")]
    pub reader_preload_count: u32,
    #[serde(default = "default_true")]
    pub auto_bookmark: bool,
}

fn default_manga_provider() -> String {
    "mangadex".to_string()
}

fn default_preload() -> u32 {
    3
}

fn default_true() -> bool {
    true
}

fn default_theme() -> ThemeConfig {
    ThemeConfig {
        primary_color: "#E50914".to_string(), // Netflix Red
        secondary_color: "#ffffff".to_string(),
    }
}

impl Default for SourcesConfig {
    fn default() -> Self {
        Self {
            allanime: true,
            kkphim: true,
            ophim: true,
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        default_theme()
    }
}

impl Default for MangaConfig {
    fn default() -> Self {
        Self {
            default_provider: default_manga_provider(),
            preferred_language: String::new(),
            image_quality: String::new(),
            reader_preload_count: default_preload(),
            auto_bookmark: true,
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let config_path = Self::get_config_path()?;
        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(config_path).context("Failed to read config file")?;
        toml::from_str(&content).context("Failed to parse config file")
    }

    pub fn save(&self) -> Result<()> {
        let config_path = Self::get_config_path()?;
        let parent = config_path.parent().context("Invalid config path")?;
        std::fs::create_dir_all(parent).context("Failed to create config directory")?;

        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(config_path, content).context("Failed to write config file")
    }

    pub fn get_config_path() -> Result<PathBuf> {
        let proj_dirs = ProjectDirs::from("com", "silent9669", "weeb-tui")
            .context("Failed to get config directory")?;
        Ok(proj_dirs.config_dir().join("config.toml"))
    }

    pub fn validate(&self) -> Result<()> {
        // Config validation - all providers now work without external dependencies
        Ok(())
    }
}
