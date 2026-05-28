use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

type ActiveTimers = Arc<Mutex<HashMap<String, tokio::task::JoinHandle<()>>>>;

static TIMERS: std::sync::LazyLock<ActiveTimers> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

const UPDATE_INTERVAL_SECS: u64 = 60;

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
    #[description = "完了時にVCで通知するチャンネル"] vc_channel: Option<serenity::ChannelId>,
    #[description = "VC参加者全員にメンションする"] notify_vc_members: Option<bool>,
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
    let notify_vc = notify_vc_members.unwrap_or(false);

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
    let cache = ctx.serenity_context().cache.clone();
    let pool = data.db.clone();
    let user_id_str = user_id.to_string();
    let guild_id_clone = guild_id.clone();
    let key_clone = key.clone();

    let manager = songbird::get(ctx.serenity_context()).await;
    let pomo_file = data.config.audio.pomo_file.clone();
    let auto_leave_timeout =
        std::time::Duration::from_secs(data.config.audio.auto_leave_timeout_sec);
    let guild_id_parsed: serenity::GuildId = guild_id.parse::<u64>().unwrap_or(0).into();

    // VC参加者のメンション文を事前に取得
    let vc_members_mention = if notify_vc {
        get_vc_members_mention(&cache, guild_id_parsed, user_id).await
    } else {
        None
    };

    let mut desc = format!("{}分間がんばりましょう！", minutes);
    if let Some(vc) = vc_channel {
        desc.push_str(&format!("\n🔊 完了時に <#{}> で通知", vc));
    }
    if let Some(ref mentions) = vc_members_mention {
        desc.push_str(&format!("\n👥 {}", mentions));
    }

    let progress_msg = channel_id
        .send_message(
            ctx.http(),
            serenity::CreateMessage::new().embed(build_progress_embed(minutes, 0)),
        )
        .await?;

    let progress_msg_id = progress_msg.id;

    let handle = tokio::spawn(async move {
        let total_secs = u64::from(minutes) * 60;
        let mut elapsed_secs: u64 = 0;

        loop {
            let sleep_dur = UPDATE_INTERVAL_SECS.min(total_secs - elapsed_secs);
            tokio::time::sleep(std::time::Duration::from_secs(sleep_dur)).await;
            elapsed_secs += sleep_dur;

            if elapsed_secs >= total_secs {
                break;
            }

            let embed = build_progress_embed(minutes, elapsed_secs);
            channel_id
                .edit_message(&http, progress_msg_id, serenity::EditMessage::new().embed(embed))
                .await
                .ok();
        }

        // 完了処理
        sqlx::query(
            "UPDATE pomodoro_sessions SET completed = 1, finished_at = datetime('now') WHERE user_id = ? AND guild_id = ? AND completed = 0",
        )
        .bind(&user_id_str)
        .bind(&guild_id_clone)
        .execute(&pool)
        .await
        .ok();

        // VC通知
        if let Some(vc) = vc_channel
            && let Some(ref mgr) = manager
            && let Err(e) = crate::voice::play_sound_in_vc(
                mgr,
                guild_id_parsed,
                vc,
                &pomo_file,
                auto_leave_timeout,
            )
            .await
        {
            tracing::warn!("pomo VC notification failed: {}", e);
        }

        // 完了embed更新
        let done_embed = CreateEmbed::new()
            .title("🍅 ポモドーロ完了！")
            .description("お疲れさまでした！休憩しましょう。")
            .color(0x57F287);
        channel_id
            .edit_message(&http, progress_msg_id, serenity::EditMessage::new().embed(done_embed))
            .await
            .ok();

        // 完了通知（メンション付き）
        let mut notify = format!("<@{}> ポモドーロ完了！🍅", user_id);
        if let Some(ref mentions) = vc_members_mention {
            notify.push_str(&format!("\n{}", mentions));
        }
        channel_id
            .send_message(&http, serenity::CreateMessage::new().content(notify))
            .await
            .ok();

        TIMERS.lock().await.remove(&key_clone);
    });

    TIMERS.lock().await.insert(key, handle);
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
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("進行中のポモドーロはありません。")
                .color(0xED4245),
        )).await?;
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
            let elapsed_secs = elapsed.num_seconds().max(0) as u64;

            ctx.send(
                poise::CreateReply::default()
                    .embed(build_progress_embed(duration as u32, elapsed_secs)),
            )
            .await?;
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

fn build_progress_embed(total_min: u32, elapsed_secs: u64) -> CreateEmbed {
    let total_secs = u64::from(total_min) * 60;
    let remaining = total_secs.saturating_sub(elapsed_secs);
    let mins = remaining / 60;
    let secs = remaining % 60;

    let ratio = if total_secs > 0 {
        elapsed_secs as f64 / total_secs as f64
    } else {
        0.0
    };
    let filled = (ratio * 20.0).round() as usize;
    let bar = format!(
        "{}{}",
        "█".repeat(filled.min(20)),
        "░".repeat(20 - filled.min(20)),
    );

    let pct = (ratio * 100.0).min(100.0);

    CreateEmbed::new()
        .title("🍅 ポモドーロ進行中")
        .field("残り時間", format!("**{}:{:02}**", mins, secs), true)
        .field("作業時間", format!("{}分", total_min), true)
        .field("進捗", format!("{} {:.0}%", bar, pct), false)
        .color(0xEB459E)
}

async fn get_vc_members_mention(
    cache: &serenity::Cache,
    guild_id: serenity::GuildId,
    exclude_user: serenity::UserId,
) -> Option<String> {
    let guild = cache.guild(guild_id)?;

    let user_vc = guild
        .voice_states
        .get(&exclude_user)
        .and_then(|vs| vs.channel_id)?;

    let mentions: Vec<String> = guild
        .voice_states
        .iter()
        .filter(|(uid, vs)| {
            **uid != exclude_user && vs.channel_id == Some(user_vc)
        })
        .map(|(uid, _)| format!("<@{}>", uid))
        .collect();

    if mentions.is_empty() {
        None
    } else {
        Some(mentions.join(" "))
    }
}
