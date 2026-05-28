use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

type ActiveTimers = Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>;

static TIMERS: std::sync::LazyLock<ActiveTimers> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

/// ポモドーロタイマー
#[poise::command(slash_command, subcommands("start", "stop", "status"))]
pub async fn pomo(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// ポモドーロを開始
#[poise::command(slash_command)]
async fn start(
    ctx: Context<'_>,
    #[description = "作業時間（分、デフォルト25）"] minutes: Option<u32>,
) -> Result<(), Error> {
    let data = ctx.data();
    let user_id = ctx.author().id;
    let key = format!("{}_{}", user_id, ctx.guild_id().unwrap_or_default());

    {
        let timers = TIMERS.lock().await;
        if timers.contains_key(&key) {
            ctx.send(
                poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .title("⚠️ ポモドーロ進行中")
                        .description("`/pomo stop` で中断できます。")
                        .color(0xFEE75C),
                ),
            )
            .await?;
            return Ok(());
        }
    }

    let minutes = minutes.unwrap_or(data.config.pomodoro.default_work_min);
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    sqlx::query(
        "INSERT INTO pomodoro_sessions (user_id, guild_id, duration_min) VALUES (?, ?, ?)",
    )
    .bind(user_id.to_string())
    .bind(&guild_id)
    .bind(minutes)
    .execute(&data.db)
    .await?;

    let channel_id = ctx.channel_id();
    let http = ctx.serenity_context().http.clone();
    let pool = data.db.clone();
    let user_id_str = user_id.to_string();
    let guild_id_clone = guild_id.clone();
    let key_clone = key.clone();

    let handle = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(u64::from(minutes) * 60)).await;

        sqlx::query(
            "UPDATE pomodoro_sessions SET completed = 1, finished_at = datetime('now') WHERE user_id = ? AND guild_id = ? AND completed = 0",
        )
        .bind(&user_id_str)
        .bind(&guild_id_clone)
        .execute(&pool)
        .await
        .ok();

        let embed = CreateEmbed::new()
            .title("🍅 ポモドーロ完了！")
            .description(format!(
                "<@{}> {}分間お疲れさまでした！\n休憩しましょう。",
                user_id, minutes
            ))
            .color(0x57F287);

        channel_id
            .send_message(&http, poise::serenity_prelude::CreateMessage::new().embed(embed))
            .await
            .ok();

        TIMERS.lock().await.remove(&key_clone);
    });

    TIMERS.lock().await.insert(key, handle);

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🍅 ポモドーロ開始")
                .description(format!("{}分間がんばりましょう！", minutes))
                .color(0xEB459E),
        ),
    )
    .await?;
    Ok(())
}

/// ポモドーロを中断
#[poise::command(slash_command)]
async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let key = format!(
        "{}_{}",
        ctx.author().id,
        ctx.guild_id().unwrap_or_default()
    );

    let mut timers = TIMERS.lock().await;
    if let Some(handle) = timers.remove(&key) {
        handle.abort();

        let data = ctx.data();
        let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
        sqlx::query(
            "UPDATE pomodoro_sessions SET finished_at = datetime('now') WHERE user_id = ? AND guild_id = ? AND completed = 0",
        )
        .bind(ctx.author().id.to_string())
        .bind(&guild_id)
        .execute(&data.db)
        .await?;

        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("⏹️ ポモドーロ中断")
                    .description("ポモドーロを中断しました。")
                    .color(0xED4245),
            ),
        )
        .await?;
    } else {
        ctx.say("進行中のポモドーロはありません。").await?;
    }
    Ok(())
}

/// ポモドーロの状態確認
#[poise::command(slash_command)]
async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let key = format!(
        "{}_{}",
        ctx.author().id,
        ctx.guild_id().unwrap_or_default()
    );

    let timers = TIMERS.lock().await;
    if timers.contains_key(&key) {
        let data = ctx.data();
        let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

        let session = sqlx::query_as::<_, (i64, String)>(
            "SELECT duration_min, started_at FROM pomodoro_sessions WHERE user_id = ? AND guild_id = ? AND completed = 0 ORDER BY started_at DESC LIMIT 1",
        )
        .bind(ctx.author().id.to_string())
        .bind(&guild_id)
        .fetch_optional(&data.db)
        .await?;

        if let Some((duration, started)) = session {
            let started_at =
                chrono::NaiveDateTime::parse_from_str(&started, "%Y-%m-%d %H:%M:%S")
                    .unwrap_or_default();
            let elapsed = chrono::Utc::now().naive_utc() - started_at;
            let remaining = duration * 60 - elapsed.num_seconds();

            if remaining > 0 {
                let mins = remaining / 60;
                let secs = remaining % 60;
                let progress = ((elapsed.num_seconds() as f64 / (duration as f64 * 60.0)) * 10.0).round() as usize;
                let bar = format!("{}{}",
                    "█".repeat(progress.min(10)),
                    "░".repeat(10 - progress.min(10)),
                );

                ctx.send(
                    poise::CreateReply::default().embed(
                        CreateEmbed::new()
                            .title("🍅 ポモドーロ進行中")
                            .field("残り時間", format!("{}分{}秒", mins, secs), true)
                            .field("作業時間", format!("{}分", duration), true)
                            .field("進捗", bar, false)
                            .color(0xEB459E),
                    ),
                )
                .await?;
            }
        }
    } else {
        let data = ctx.data();
        let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM pomodoro_sessions WHERE user_id = ? AND guild_id = ? AND completed = 1 AND date(started_at) = date('now')",
        )
        .bind(ctx.author().id.to_string())
        .bind(&guild_id)
        .fetch_one(&data.db)
        .await?;

        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🍅 ポモドーロ")
                    .description("進行中のポモドーロはありません。")
                    .field("今日の完了セッション", format!("{}回", count), true)
                    .color(0x99AAB5),
            ),
        )
        .await?;
    }
    Ok(())
}
