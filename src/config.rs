use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub bot: BotConfig,
    pub pomodoro: PomodoroConfig,
    pub diary: DiaryConfig,
    pub database: DatabaseConfig,
    pub audio: AudioConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BotConfig {
    pub default_ai_provider: String,
    pub default_model: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PomodoroConfig {
    pub default_work_min: u32,
    pub default_break_min: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DiaryConfig {
    pub template_fields: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AudioConfig {
    pub alarm_file: String,
    pub pomo_file: String,
    pub auto_leave_timeout_sec: u64,
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Config = toml::from_str(&content)?;
        Ok(config)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bot: BotConfig {
                default_ai_provider: "openrouter".to_string(),
                default_model: "openai/gpt-4o-mini".to_string(),
            },
            pomodoro: PomodoroConfig {
                default_work_min: 25,
                default_break_min: 5,
            },
            diary: DiaryConfig {
                template_fields: vec![
                    "やったこと".to_string(),
                    "明日やること".to_string(),
                    "感想".to_string(),
                ],
            },
            database: DatabaseConfig {
                path: "data/quid.db".to_string(),
            },
            audio: AudioConfig {
                alarm_file: "assets/alarm.mp3".to_string(),
                pomo_file: "assets/pomo.mp3".to_string(),
                auto_leave_timeout_sec: 30,
            },
        }
    }
}
