use crate::{
    error::{AppError, AppResult},
    model::AppConfig,
};

use directories::ProjectDirs;

use std::{fs, path::PathBuf, time::SystemTime};

const DISPLAY_WIDTH: usize = 14;
const MIN_WRITE_STEP_MS: u64 = 20;
const MIN_SCROLL_INTERVAL_MS: u64 = 100;
const MIN_FULL_REFRESH_SECONDS: u64 = 5;
const MAX_TITLE_GAP: usize = 32;

pub struct ConfigManager;

impl ConfigManager {
    pub fn directory() -> AppResult<PathBuf> {
        let directories =
            ProjectDirs::from("com", "nightshade", "nightshade_mp3").ok_or_else(|| {
                AppError::Message("Could not resolve the configuration directory.".to_owned())
            })?;

        Ok(directories.config_dir().to_path_buf())
    }

    pub fn path() -> AppResult<PathBuf> {
        Ok(Self::directory()?.join("config.json"))
    }

    pub fn load() -> AppResult<AppConfig> {
        let path = Self::path()?;

        if !path.exists() {
            let config = Self::normalize(AppConfig::default());

            Self::save(&config)?;

            return Ok(config);
        }

        let text = fs::read_to_string(path)?;

        let config = serde_json::from_str::<AppConfig>(&text)?;

        Ok(Self::normalize(config))
    }

    pub fn save(config: &AppConfig) -> AppResult<()> {
        let directory = Self::directory()?;

        fs::create_dir_all(&directory)?;

        let path = Self::path()?;

        let temporary = directory.join("config.json.tmp");

        let config = Self::normalize(config.clone());

        let text = serde_json::to_string_pretty(&config)?;

        fs::write(&temporary, text)?;

        if path.exists() {
            fs::remove_file(&path)?;
        }

        fs::rename(temporary, path)?;

        Ok(())
    }

    pub fn modified_time() -> AppResult<Option<SystemTime>> {
        let path = Self::path()?;

        if !path.exists() {
            return Ok(None);
        }

        Ok(Some(fs::metadata(path)?.modified()?))
    }

    pub fn load_if_changed(previous: &mut Option<SystemTime>) -> AppResult<Option<AppConfig>> {
        let current = Self::modified_time()?;

        if current.is_none() || current == *previous {
            return Ok(None);
        }

        let config = Self::load()?;

        *previous = current;

        Ok(Some(config))
    }

    fn normalize(mut config: AppConfig) -> AppConfig {
        config.text.width = DISPLAY_WIDTH;

        config.text.write_step_ms = config.text.write_step_ms.max(MIN_WRITE_STEP_MS);

        config.text.scroll_interval_ms = config.text.scroll_interval_ms.max(MIN_SCROLL_INTERVAL_MS);

        config.text.full_refresh_seconds = config
            .text
            .full_refresh_seconds
            .max(MIN_FULL_REFRESH_SECONDS);

        config.text.title_gap = config.text.title_gap.clamp(1, MAX_TITLE_GAP);

        if config.text.separator.chars().count() > 8 {
            config.text.separator = config.text.separator.chars().take(8).collect();
        }

        config
    }
}
