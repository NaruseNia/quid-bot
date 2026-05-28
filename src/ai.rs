use sqlx::SqlitePool;

pub struct AiConfig {
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

pub async fn resolve(
    db: &SqlitePool,
    guild_id: &str,
    config: &crate::config::Config,
    provider_override: Option<&str>,
    model_override: Option<String>,
) -> AiConfig {
    let provider = if let Some(p) = provider_override {
        p.to_string()
    } else {
        get_guild_setting(db, guild_id, "ai_provider")
            .await
            .unwrap_or_else(|| config.bot.default_ai_provider.clone())
    };

    let default_model = get_guild_setting(db, guild_id, "ai_model")
        .await
        .unwrap_or_else(|| config.bot.default_model.clone());

    match provider.as_str() {
        "openai" => {
            let api_key = get_encrypted_setting(db, guild_id, "openai_api_key")
                .await
                .or_else(|| std::env::var("OPENAI_API_KEY").ok())
                .unwrap_or_default();
            let model = model_override.unwrap_or_else(|| strip_provider_prefix(&default_model));
            AiConfig {
                api_url: "https://api.openai.com/v1/chat/completions".to_string(),
                api_key,
                model,
            }
        }
        "anthropic" => {
            let api_key = get_encrypted_setting(db, guild_id, "openrouter_api_key")
                .await
                .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
                .unwrap_or_default();
            let model = model_override.unwrap_or_else(|| {
                if default_model.starts_with("anthropic/") {
                    default_model.clone()
                } else {
                    "anthropic/claude-sonnet-4-20250514".to_string()
                }
            });
            AiConfig {
                api_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
                api_key,
                model,
            }
        }
        _ => {
            let api_key = get_encrypted_setting(db, guild_id, "openrouter_api_key")
                .await
                .or_else(|| std::env::var("OPENROUTER_API_KEY").ok())
                .unwrap_or_default();
            let model = model_override.unwrap_or(default_model);
            AiConfig {
                api_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
                api_key,
                model,
            }
        }
    }
}

fn strip_provider_prefix(model: &str) -> String {
    model.split('/').next_back().unwrap_or(model).to_string()
}

async fn get_guild_setting(db: &SqlitePool, guild_id: &str, key: &str) -> Option<String> {
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM guild_settings WHERE guild_id = ? AND key = ?",
    )
    .bind(guild_id)
    .bind(key)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
}

async fn get_encrypted_setting(db: &SqlitePool, guild_id: &str, key: &str) -> Option<String> {
    let encrypted = get_guild_setting(db, guild_id, key).await?;
    match crate::crypto::decrypt(&encrypted) {
        Ok(plaintext) => Some(plaintext),
        Err(e) => {
            tracing::warn!("failed to decrypt setting {} for guild {}: {}", key, guild_id, e);
            None
        }
    }
}

pub async fn set_guild_setting(
    db: &SqlitePool,
    guild_id: &str,
    key: &str,
    value: &str,
) -> Result<(), crate::error::Error> {
    sqlx::query(
        "INSERT INTO guild_settings (guild_id, key, value) VALUES (?, ?, ?) ON CONFLICT(guild_id, key) DO UPDATE SET value = excluded.value",
    )
    .bind(guild_id)
    .bind(key)
    .bind(value)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn set_encrypted_setting(
    db: &SqlitePool,
    guild_id: &str,
    key: &str,
    plaintext: &str,
) -> Result<(), crate::error::Error> {
    let encrypted = crate::crypto::encrypt(plaintext)?;
    set_guild_setting(db, guild_id, key, &encrypted).await
}
