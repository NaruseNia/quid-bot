use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;

/// サーバー設定
#[poise::command(
    slash_command,
    subcommands("apikey", "provider", "model", "show_settings"),
    required_permissions = "MANAGE_GUILD"
)]
pub async fn settings(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// APIキーを設定（サーバー単位）
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD", ephemeral)]
async fn apikey(
    ctx: Context<'_>,
    #[description = "プロバイダー (openai/openrouter)"] provider: String,
    #[description = "APIキー"] key: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let setting_key = match provider.to_lowercase().as_str() {
        "openai" => "openai_api_key",
        "openrouter" => "openrouter_api_key",
        _ => {
            ctx.say("プロバイダーは `openai` または `openrouter` を指定してください。")
                .await?;
            return Ok(());
        }
    };

    crate::ai::set_guild_setting(&data.db, &guild_id, setting_key, &key).await?;

    let masked = if key.len() > 8 {
        format!("{}...{}", &key[..4], &key[key.len() - 4..])
    } else {
        "****".to_string()
    };

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🔑 APIキー設定完了")
                .description(format!(
                    "**{}** のAPIキーを設定しました。\nキー: `{}`",
                    provider, masked
                ))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// AIプロバイダーを設定（サーバー単位）
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD")]
async fn provider(
    ctx: Context<'_>,
    #[description = "プロバイダー (openai/openrouter/anthropic)"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let valid = ["openai", "openrouter", "anthropic"];
    if !valid.contains(&name.to_lowercase().as_str()) {
        ctx.say(format!(
            "プロバイダーは {} のいずれかを指定してください。",
            valid
                .iter()
                .map(|v| format!("`{}`", v))
                .collect::<Vec<_>>()
                .join(" / ")
        ))
        .await?;
        return Ok(());
    }

    crate::ai::set_guild_setting(&data.db, &guild_id, "ai_provider", &name.to_lowercase())
        .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("⚙️ プロバイダー設定")
                .description(format!("AIプロバイダーを **{}** に設定しました。", name))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// AIモデルを設定（サーバー単位）
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD")]
async fn model(
    ctx: Context<'_>,
    #[description = "モデル名 (例: gpt-4o-mini, gpt-4o, openai/gpt-4o-mini)"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    crate::ai::set_guild_setting(&data.db, &guild_id, "ai_model", &name).await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("⚙️ モデル設定")
                .description(format!("AIモデルを **{}** に設定しました。", name))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// 現在のサーバー設定を表示
#[poise::command(slash_command, rename = "show", required_permissions = "MANAGE_GUILD")]
async fn show_settings(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT key, value FROM guild_settings WHERE guild_id = ? AND key IN ('ai_provider', 'ai_model', 'openai_api_key', 'openrouter_api_key', 'weather_city', 'today_feeds') ORDER BY key",
    )
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    let global_provider = &data.config.bot.default_ai_provider;
    let global_model = &data.config.bot.default_model;

    let mut desc = format!(
        "**グローバル設定** (config.toml)\nプロバイダー: `{}`\nモデル: `{}`\n\n",
        global_provider, global_model
    );

    if rows.is_empty() {
        desc.push_str("**サーバー固有設定**: なし（グローバル設定を使用）");
    } else {
        desc.push_str("**サーバー固有設定**\n");
        for (key, value) in &rows {
            let display = if key.contains("api_key") {
                if value.len() > 8 {
                    format!("{}...{}", &value[..4], &value[value.len() - 4..])
                } else {
                    "****".to_string()
                }
            } else {
                value.clone()
            };
            desc.push_str(&format!("`{}` = `{}`\n", key, display));
        }
    }

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("⚙️ サーバー設定")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}
