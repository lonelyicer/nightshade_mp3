use serde::{Deserialize, Serialize};

use std::time::Instant;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub osc: OscConfig,
    pub oscquery: OscQueryConfig,
    pub text: TextConfig,
    pub parameters: ParameterConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            osc: OscConfig::default(),
            oscquery: OscQueryConfig::default(),
            text: TextConfig::default(),
            parameters: ParameterConfig::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OscConfig {
    pub host: String,
    pub port: u16,
    pub auto_discover: bool,
}

impl Default for OscConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 9000,
            auto_discover: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OscQueryConfig {
    pub enabled: bool,
}

impl Default for OscQueryConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TextConfig {
    pub width: usize,

    #[serde(alias = "update_interval_ms")]
    pub write_step_ms: u64,

    pub scroll_interval_ms: u64,
    pub title_gap: usize,
    pub separator: String,
}

impl Default for TextConfig {
    fn default() -> Self {
        Self {
            width: 14,
            write_step_ms: 250,
            scroll_interval_ms: 2000,
            title_gap: 5,
            separator: " - ".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ParameterConfig {
    pub pointer: String,
    pub character: String,
}

impl Default for ParameterConfig {
    fn default() -> Self {
        Self {
            pointer: "TA_Pointer".to_owned(),
            character: "TA_Char".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
}

impl MediaInfo {
    pub fn is_available(&self) -> bool {
        !self.title.trim().is_empty() || !self.artist.trim().is_empty()
    }

    pub fn display_name(&self, separator: &str) -> String {
        let title = self.title.trim();
        let artist = self.artist.trim();

        match (title.is_empty(), artist.is_empty()) {
            (false, false) => {
                format!("{title}{separator}{artist}")
            }

            (false, true) => title.to_owned(),
            (true, false) => artist.to_owned(),
            (true, true) => String::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MediaState {
    pub info: MediaInfo,
    pub updated_at: Instant,
}

impl Default for MediaState {
    fn default() -> Self {
        Self {
            info: MediaInfo::default(),
            updated_at: Instant::now(),
        }
    }
}

impl MediaState {
    pub fn update(&mut self, info: MediaInfo) {
        self.info = info;
        self.updated_at = Instant::now();
    }

    pub fn current_position(&self) -> f64 {
        let mut position = self.info.position;

        if self.info.playing {
            position += self.updated_at.elapsed().as_secs_f64();
        }

        if self.info.duration > 0.0 {
            position.clamp(0.0, self.info.duration)
        } else {
            position.max(0.0)
        }
    }
}
