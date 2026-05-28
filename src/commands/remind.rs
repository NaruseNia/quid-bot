use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// リマインダー
#[poise::command(slash_command, subcommands("set", "repeat", "list", "delete"))]
pub async fn remind(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 一回限りのリマインダーを設定
#[poise::command(slash_command)]
async fn set(
    ctx: Context<'_>,
    #[description = "時間 (例: 30m, 2h, 1d, 2025-06-01 09:00)"] time: String,
    #[description = "メッセージ"] message: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let remind_at = parse_time(&time)?;

    sqlx::query(
        "INSERT INTO reminders (user_id, guild_id, channel_id, message, remind_at, is_recurring) VALUES (?, ?, ?, ?, ?, 0)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(ctx.channel_id().to_string())
    .bind(&message)
    .bind(remind_at.format("%Y-%m-%d %H:%M:%S").to_string())
    .execute(&data.db)
    .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("⏰ リマインダー設定")
                .color(0x57F287)
                .field("時刻", remind_at.format("%Y-%m-%d %H:%M").to_string(), true)
                .field("メッセージ", &message, true),
        ),
    )
    .await?;
    Ok(())
}

/// 繰り返しリマインダーを設定
#[poise::command(slash_command)]
async fn repeat(
    ctx: Context<'_>,
    #[description = "頻度 (daily, weekly, monthly)"] frequency: String,
    #[description = "時刻 (HH:MM)"] time: String,
    #[description = "メッセージ"] message: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let cron_expr = match frequency.to_lowercase().as_str() {
        "daily" => format!("0 {} * * *", time.replace(':', " ")),
        "weekly" => format!("0 {} * * 1", time.replace(':', " ")),
        "monthly" => format!("0 {} 1 * *", time.replace(':', " ")),
        _ => {
            ctx.say("頻度は daily, weekly, monthly のいずれかを指定してください。")
                .await?;
            return Ok(());
        }
    };

    let now = chrono::Local::now();
    let parts: Vec<&str> = time.split(':').collect();
    let hour: u32 = parts.first().and_then(|h| h.parse().ok()).unwrap_or(9);
    let min: u32 = parts.get(1).and_then(|m| m.parse().ok()).unwrap_or(0);

    let mut remind_at = now
        .date_naive()
        .and_hms_opt(hour, min, 0)
        .unwrap_or_default();
    if remind_at <= now.naive_local() {
        remind_at += chrono::Duration::days(1);
    }

    sqlx::query(
        "INSERT INTO reminders (user_id, guild_id, channel_id, message, remind_at, cron_expr, is_recurring) VALUES (?, ?, ?, ?, ?, ?, 1)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(ctx.channel_id().to_string())
    .bind(&message)
    .bind(remind_at.format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(&cron_expr)
    .execute(&data.db)
    .await?;

    let freq_ja = match frequency.to_lowercase().as_str() {
        "daily" => "毎日",
        "weekly" => "毎週",
        "monthly" => "毎月",
        _ => &frequency,
    };

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🔁 繰り返しリマインダー設定")
                .color(0x57F287)
                .field("頻度", freq_ja, true)
                .field("時刻", &time, true)
                .field("メッセージ", &message, false),
        ),
    )
    .await?;
    Ok(())
}

/// リマインダー一覧
#[poise::command(slash_command)]
async fn list(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let reminders = sqlx::query_as::<_, (i64, String, String, bool)>(
        "SELECT id, message, remind_at, is_recurring FROM reminders WHERE user_id = ? AND guild_id = ? AND is_active = 1 ORDER BY remind_at ASC",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if reminders.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("⏰ リマインダー一覧")
                    .description("アクティブなリマインダーはありません。")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return Ok(());
    }

    let desc: String = reminders
        .iter()
        .map(|(id, message, remind_at, is_recurring)| {
            let icon = if *is_recurring { "🔁" } else { "⏰" };
            format!("{} **#{}** | {} — {}", icon, id, remind_at, message)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("⏰ リマインダー一覧")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

/// リマインダーを削除
#[poise::command(slash_command)]
async fn delete(ctx: Context<'_>, #[description = "リマインダーID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();
    let result =
        sqlx::query("UPDATE reminders SET is_active = 0 WHERE id = ? AND user_id = ?")
            .bind(id)
            .bind(ctx.author().id.to_string())
            .execute(&data.db)
            .await?;

    if result.rows_affected() == 0 {
        ctx.say("該当するリマインダーが見つかりません。").await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🗑️ リマインダー削除")
                    .description(format!("リマインダー #{} を削除しました。", id))
                    .color(0xED4245),
            ),
        )
        .await?;
    }
    Ok(())
}

pub fn parse_time_public(input: &str) -> Result<chrono::NaiveDateTime, Error> {
    parse_time(input)
}

fn parse_time(input: &str) -> Result<chrono::NaiveDateTime, Error> {
    let now = chrono::Local::now().naive_local();

    if let Some(mins) = input.strip_suffix('m') {
        let mins: i64 = mins.parse().map_err(|_| "分数が不正")?;
        return Ok(now + chrono::Duration::minutes(mins));
    }
    if let Some(hours) = input.strip_suffix('h') {
        let hours: i64 = hours.parse().map_err(|_| "時間数が不正")?;
        return Ok(now + chrono::Duration::hours(hours));
    }
    if let Some(days) = input.strip_suffix('d') {
        let days: i64 = days.parse().map_err(|_| "日数が不正")?;
        return Ok(now + chrono::Duration::days(days));
    }

    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M") {
        return Ok(dt);
    }

    if let Ok(t) = chrono::NaiveTime::parse_from_str(input, "%H:%M") {
        let mut dt = now.date().and_time(t);
        if dt <= now {
            dt += chrono::Duration::days(1);
        }
        return Ok(dt);
    }

    Err("時間形式が不正。例: 30m, 2h, 1d, 15:00, 2025-06-01 09:00".into())
}

pub async fn reminder_loop(http: std::sync::Arc<serenity::Http>, pool: sqlx::SqlitePool) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(30)).await;

        let now = chrono::Local::now()
            .naive_local()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();

        let due = sqlx::query_as::<_, (i64, String, String, String, bool, Option<String>)>(
            "SELECT id, user_id, channel_id, message, is_recurring, cron_expr FROM reminders WHERE is_active = 1 AND remind_at <= ?",
        )
        .bind(&now)
        .fetch_all(&pool)
        .await;

        let Ok(due) = due else { continue };

        for (id, user_id, channel_id, message, is_recurring, _cron_expr) in due {
            let channel: serenity::ChannelId = channel_id.parse::<u64>().unwrap_or(0).into();

            let embed = CreateEmbed::new()
                .title("⏰ リマインダー")
                .description(format!("<@{}>\n{}", user_id, message))
                .color(0xFEE75C);

            channel
                .send_message(&http, serenity::CreateMessage::new().embed(embed))
                .await
                .ok();

            if is_recurring {
                let next = chrono::Local::now().naive_local() + chrono::Duration::days(1);
                sqlx::query("UPDATE reminders SET remind_at = ? WHERE id = ?")
                    .bind(next.format("%Y-%m-%d %H:%M:%S").to_string())
                    .bind(id)
                    .execute(&pool)
                    .await
                    .ok();
            } else {
                sqlx::query("UPDATE reminders SET is_active = 0 WHERE id = ?")
                    .bind(id)
                    .execute(&pool)
                    .await
                    .ok();
            }
        }
    }
}
