use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// VCアラーム
#[poise::command(slash_command, subcommands("set", "list_alarms", "delete"))]
pub async fn alarm(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// アラームを設定
#[poise::command(slash_command)]
async fn set(
    ctx: Context<'_>,
    #[description = "時刻 (例: 30m, 2h, 15:00, 2025-06-01 09:00)"] time: String,
    #[description = "VCチャンネル"] channel: serenity::ChannelId,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let alarm_at = super::remind::parse_time_public(&time)?;

    sqlx::query(
        "INSERT INTO alarms (user_id, guild_id, channel_id, alarm_at) VALUES (?, ?, ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(channel.to_string())
    .bind(alarm_at.format("%Y-%m-%d %H:%M:%S").to_string())
    .execute(&data.db)
    .await?;

    let embed = CreateEmbed::new()
        .title("⏰ アラーム設定完了")
        .color(0x57F287)
        .field("時刻", alarm_at.format("%Y-%m-%d %H:%M").to_string(), true)
        .field("チャンネル", format!("<#{}>", channel), true);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// アラーム一覧
#[poise::command(slash_command, rename = "list")]
async fn list_alarms(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let alarms = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, channel_id, alarm_at FROM alarms WHERE user_id = ? AND guild_id = ? AND is_active = 1 ORDER BY alarm_at ASC",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if alarms.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("⏰ アラーム一覧")
                    .description("アクティブなアラームはありません。")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return Ok(());
    }

    let mut desc = String::new();
    for (id, channel_id, alarm_at) in &alarms {
        desc.push_str(&format!("**#{}** | {} — <#{}>\n", id, alarm_at, channel_id));
    }

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("⏰ アラーム一覧")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

/// アラームを削除
#[poise::command(slash_command)]
async fn delete(ctx: Context<'_>, #[description = "アラームID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();
    let result =
        sqlx::query("UPDATE alarms SET is_active = 0 WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(ctx.author().id.to_string())
            .execute(&data.db)
            .await?;

    if result.rows_affected() == 0 {
        ctx.say("該当するアラームが見つかりません。").await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🗑️ アラーム削除")
                    .description(format!("アラーム #{} を削除しました。", id))
                    .color(0xED4245),
            ),
        )
        .await?;
    }
    Ok(())
}
