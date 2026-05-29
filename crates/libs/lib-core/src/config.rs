use std::sync::OnceLock;

use anyhow::Context;

// region:    --- Config

pub fn config() -> &'static AppConfig {
    static INSTANCE: OnceLock<AppConfig> = OnceLock::new();

    INSTANCE.get_or_init(|| {
        AppConfig::load_from_env()
            .unwrap_or_else(|ex| panic!("FATAL - WHILE LOADING CONFIG - Cause: {ex:?}"))
    })
}

#[allow(non_snake_case)]
#[derive(Debug)]
pub struct AppConfig {
    pub DATABASE_URL: String,
    pub PSP_BASE_URL: String,
    pub PORT: u16,
    pub DB_MAX_CONNECTIONS: u32,
}

impl AppConfig {
    fn load_from_env() -> anyhow::Result<Self> {
        Ok(Self {
            DATABASE_URL: std::env::var("DATABASE_URL").context("DATABASE_URL must be set")?,
            PSP_BASE_URL: std::env::var("PSP_BASE_URL")
                .unwrap_or_else(|_| "http://mock-psp:9090".to_string()),
            PORT: std::env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .context("PORT must be a valid u16")?,
            DB_MAX_CONNECTIONS: std::env::var("DB_MAX_CONNECTIONS")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .context("DB_MAX_CONNECTIONS must be a valid u32")?,
        })
    }
}

// endregion: --- Config
