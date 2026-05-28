use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// VCアラーム
#[poise::command(slash_command, subcommands("set", "list_alarms", "delete", "snooze", "stop", "volume"))]
pub async fn alarm(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// アラームを設定
#[poise::command(slash_command)]
async fn set(
    ctx: Context<'_>,
    #[description = "時刻 (例: 30m, 2h, 15:00, 2025-06-01 09:00)"] time: String,
    #[description = "VCチャンネル（省略時は現在のVC）"] channel: Option<serenity::ChannelId>,
    #[description = "リピート回数（デフォルト3）"] repeat: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let repeat_count = repeat.unwrap_or(3).max(1).min(20);

    let vc_channel = if let Some(ch) = channel {
        ch
    } else if let Some(gid) = ctx.guild_id() {
        let guild = ctx.serenity_context().cache.guild(gid);
        let vc = guild.and_then(|g| {
            g.voice_states
                .get(&ctx.author().id)
                .and_then(|vs| vs.channel_id)
        });
        match vc {
            Some(ch) => ch,
            None => {
                ctx.send(poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .description("VCに参加しているか、チャンネルを指定してください。")
                        .color(0xED4245),
                )).await?;
                return Ok(());
            }
        }
    } else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("VCチャンネルを指定してください。")
                .color(0xED4245),
        )).await?;
        return Ok(());
    };

    let alarm_at = super::remind::parse_time_public(&time)?;

    sqlx::query(
        "INSERT INTO alarms (user_id, guild_id, channel_id, alarm_at, repeat_count) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(vc_channel.to_string())
    .bind(alarm_at.format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(repeat_count)
    .execute(&data.db)
    .await?;

    let embed = CreateEmbed::new()
        .title("⏰ アラーム設定完了")
        .color(0x57F287)
        .field("時刻", alarm_at.format("%Y-%m-%d %H:%M").to_string(), true)
        .field("チャンネル", format!("<#{}>", vc_channel), true)
        .field("リピート", format!("{}回", repeat_count), true);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// アラーム一覧
#[poise::command(slash_command, rename = "list")]
async fn list_alarms(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let alarms = sqlx::query_as::<_, (i64, String, String, i64, bool)>(
        "SELECT id, channel_id, alarm_at, repeat_count, ringing FROM alarms WHERE user_id = ? AND guild_id = ? AND (is_active = 1 OR ringing = 1) ORDER BY alarm_at ASC",
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
    for (id, channel_id, alarm_at, repeat_count, ringing) in &alarms {
        let status = if *ringing { "🔔" } else { "⏰" };
        desc.push_str(&format!(
            "{} **#{}** | {} — <#{}> ({}回)\n",
            status, id, alarm_at, channel_id, repeat_count
        ));
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
        sqlx::query("UPDATE alarms SET is_active = 0, ringing = 0 WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(ctx.author().id.to_string())
            .execute(&data.db)
            .await?;

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("該当するアラームが見つかりません。")
                .color(0xED4245),
        )).await?;
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

/// アラームをスヌーズ（5分後に再通知）
#[poise::command(slash_command)]
async fn snooze(
    ctx: Context<'_>,
    #[description = "アラームID"] id: i64,
    #[description = "スヌーズ時間（分、デフォルト5）"] minutes: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let minutes = minutes.unwrap_or(5);
    let new_time = chrono::Local::now().naive_local() + chrono::Duration::minutes(minutes);

    let result = sqlx::query(
        "UPDATE alarms SET alarm_at = ?, is_active = 1, ringing = 0 WHERE id = ? AND user_id = ?",
    )
    .bind(new_time.format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(id)
    .bind(ctx.author().id.to_string())
    .execute(&data.db)
    .await?;

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("該当するアラームが見つかりません。")
                .color(0xED4245),
        )).await?;
    } else {
        if let Some(manager) = songbird::get(ctx.serenity_context()).await {
            if let Some(gid) = ctx.guild_id() {
                crate::voice::leave_vc(&manager, gid).await;
            }
        }

        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("💤 スヌーズ設定")
                    .description(format!(
                        "アラーム #{} を{}分後に再設定しました。\n再通知: {}",
                        id,
                        minutes,
                        new_time.format("%H:%M")
                    ))
                    .color(0xFEE75C),
            ),
        )
        .await?;
    }
    Ok(())
}

/// アラーム音量を設定（サーバー単位、0〜100）
#[poise::command(slash_command)]
async fn volume(
    ctx: Context<'_>,
    #[description = "音量 0〜100（デフォルト30）"] level: i64,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let level = level.max(0).min(100);

    sqlx::query(
        "INSERT INTO guild_settings (guild_id, key, value) VALUES (?, 'alarm_volume', ?) ON CONFLICT(guild_id, key) DO UPDATE SET value = excluded.value",
    )
    .bind(&guild_id)
    .bind(level.to_string())
    .execute(&data.db)
    .await?;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("🔊 アラーム音量設定")
            .description(format!("音量を **{}%** に設定しました。", level))
            .color(0x57F287),
    )).await?;
    Ok(())
}

/// アラームを停止（鳴動中・スヌーズ中を含む）
#[poise::command(slash_command)]
async fn stop(
    ctx: Context<'_>,
    #[description = "アラームID（省略で全停止）"] id: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let user_id = ctx.author().id.to_string();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let result = if let Some(id) = id {
        sqlx::query(
            "UPDATE alarms SET is_active = 0, ringing = 0 WHERE id = ? AND user_id = ?",
        )
        .bind(id)
        .bind(&user_id)
        .execute(&data.db)
        .await?
    } else {
        sqlx::query(
            "UPDATE alarms SET is_active = 0, ringing = 0 WHERE user_id = ? AND guild_id = ? AND (is_active = 1 OR ringing = 1)",
        )
        .bind(&user_id)
        .bind(&guild_id)
        .execute(&data.db)
        .await?
    };

    if let Some(manager) = songbird::get(ctx.serenity_context()).await {
        if let Some(gid) = ctx.guild_id() {
            crate::voice::leave_vc(&manager, gid).await;
        }
    }

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("停止するアラームが見つかりません。")
                .color(0xED4245),
        )).await?;
    } else {
        let desc = if let Some(id) = id {
            format!("アラーム #{} を停止しました。", id)
        } else {
            format!("{}件のアラームを停止しました。", result.rows_affected())
        };
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🛑 アラーム停止")
                    .description(desc)
                    .color(0x57F287),
            ),
        )
        .await?;
    }
    Ok(())
}

pub async fn alarm_loop(
    http: std::sync::Arc<serenity::Http>,
    cache: std::sync::Arc<serenity::Cache>,
    pool: sqlx::SqlitePool,
    manager: std::sync::Arc<songbird::Songbird>,
    audio_path: String,
    auto_leave_timeout: std::time::Duration,
) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        let now = chrono::Local::now()
            .naive_local()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let due = sqlx::query_as::<_, (i64, String, String, String, i64)>(
            "SELECT id, user_id, guild_id, channel_id, repeat_count FROM alarms WHERE is_active = 1 AND alarm_at <= ?",
        )
        .bind(&now)
        .fetch_all(&pool)
        .await;

        let Ok(due) = due else { continue };

        for (id, user_id, guild_id_str, channel_id_str, repeat_count) in due {
            let guild_id: serenity::GuildId = guild_id_str.parse::<u64>().unwrap_or(0).into();
            let user_id_parsed: serenity::UserId =
                user_id.parse::<u64>().unwrap_or(0).into();

            let vc_channel = get_user_current_vc(&cache, guild_id, user_id_parsed)
                .unwrap_or_else(|| channel_id_str.parse::<u64>().unwrap_or(0).into());

            sqlx::query("UPDATE alarms SET is_active = 0, ringing = 1 WHERE id = ?")
                .bind(id)
                .execute(&pool)
                .await
                .ok();

            let text_channels = http.get_channels(guild_id).await.unwrap_or_default();

            if let Some(ch) = text_channels
                .iter()
                .find(|c| c.kind == serenity::ChannelType::Text)
            {
                let embed = CreateEmbed::new()
                    .title("⏰ アラーム！")
                    .description(format!(
                        "<@{}> アラームの時間です！\n`/alarm stop {}` で停止、`/alarm snooze {}` でスヌーズできます。",
                        user_id, id, id
                    ))
                    .color(0xED4245);

                ch.id
                    .send_message(&http, serenity::CreateMessage::new().embed(embed))
                    .await
                    .ok();
            }

            let volume = sqlx::query_scalar::<_, String>(
                "SELECT value FROM guild_settings WHERE guild_id = ? AND key = 'alarm_volume'",
            )
            .bind(guild_id.to_string())
            .fetch_optional(&pool)
            .await
            .ok()
            .flatten()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(30.0)
                / 100.0;

            let manager_clone = manager.clone();
            let pool_clone = pool.clone();
            let audio_clone = audio_path.clone();
            let timeout = auto_leave_timeout;

            tokio::spawn(async move {
                let repeats = repeat_count.max(1) as u32;
                for i in 0..repeats {
                    let still_ringing = sqlx::query_scalar::<_, bool>(
                        "SELECT ringing FROM alarms WHERE id = ?",
                    )
                    .bind(id)
                    .fetch_optional(&pool_clone)
                    .await
                    .ok()
                    .flatten()
                    .unwrap_or(false);

                    if !still_ringing {
                        break;
                    }

                    if let Err(e) = crate::voice::play_sound_once(
                        &manager_clone,
                        guild_id,
                        vc_channel,
                        &audio_clone,
                        volume,
                    )
                    .await
                    {
                        tracing::warn!("alarm #{} repeat playback failed: {}", id, e);
                        break;
                    }

                    if i < repeats - 1 {
                        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                    }
                }

                sqlx::query("UPDATE alarms SET ringing = 0 WHERE id = ?")
                    .bind(id)
                    .execute(&pool_clone)
                    .await
                    .ok();

                tokio::time::sleep(timeout).await;
                manager_clone.leave(guild_id).await.ok();
            });
        }
    }
}

fn get_user_current_vc(
    cache: &serenity::Cache,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
) -> Option<serenity::ChannelId> {
    let guild = cache.guild(guild_id)?;
    guild
        .voice_states
        .get(&user_id)
        .and_then(|vs| vs.channel_id)
}
