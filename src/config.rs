use crate::{
    error::{AppError, AppResult},
    model::AppConfig,
};
use directories::ProjectDirs;
use std::{fs, path::PathBuf, time::SystemTime};

pub struct ConfigManager;

impl ConfigManager {
    pub fn directory() -> AppResult<PathBuf> {
        let directories = ProjectDirs::from("im", "ringlo", "nightshade_mp3").ok_or_else(|| {
            AppError::Message(
                "could not resolve the application configuration directory".to_owned(),
            )
        })?;

        Ok(directories.config_dir().to_path_buf())
    }

    pub fn path() -> AppResult<PathBuf> {
        Ok(Self::directory()?.join("config.json"))
    }

    pub fn load() -> AppResult<AppConfig> {
        let path = Self::path()?;

        if !path.exists() {
            let config = AppConfig::default();
            Self::save(&config)?;
            return Ok(config);
        }

        let content = fs::read_to_string(path)?;
        let mut config: AppConfig = serde_json::from_str(&content)?;

        config.text.width = 14;
        config.text.update_interval_ms = config.text.update_interval_ms.max(50);
        config.text.scroll_interval_ms = config.text.scroll_interval_ms.max(100);
        config.text.full_refresh_seconds = config.text.full_refresh_seconds.max(5);

        Ok(config)
    }

    pub fn save(config: &AppConfig) -> AppResult<()> {
        let directory = Self::directory()?;
        fs::create_dir_all(&directory)?;

        let path = Self::path()?;
        let temporary_path = directory.join("config.json.tmp");

        let mut normalized = config.clone();
        normalized.text.width = 14;
        normalized.text.update_interval_ms = normalized.text.update_interval_ms.max(50);
        normalized.text.scroll_interval_ms = normalized.text.scroll_interval_ms.max(100);
        normalized.text.full_refresh_seconds = normalized.text.full_refresh_seconds.max(5);

        let content = serde_json::to_string_pretty(&normalized)?;

        fs::write(&temporary_path, content)?;

        if path.exists() {
            fs::remove_file(&path)?;
        }

        fs::rename(temporary_path, path)?;

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
}
