use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum SleepQuality {
    #[name = "good"]
    Good,
    #[name = "ok"]
    Ok,
    #[name = "bad"]
    Bad,
}

impl SleepQuality {
    fn as_str(self) -> &'static str {
        match self {
            SleepQuality::Good => "good",
            SleepQuality::Ok => "ok",
            SleepQuality::Bad => "bad",
        }
    }

    fn emoji(self) -> &'static str {
        match self {
            SleepQuality::Good => "😊",
            SleepQuality::Ok => "😐",
            SleepQuality::Bad => "😢",
        }
    }
}

fn quality_emoji(q: &str) -> &'static str {
    match q {
        "good" => "😊",
        "ok" => "😐",
        "bad" => "😢",
        _ => "❓",
    }
}

/// 睡眠記録
#[poise::command(
    slash_command,
    subcommands("start", "end", "log", "stats", "goal", "history")
)]
pub async fn sleep(_ctx: Context<'_>) -> Result<(), Error> {
    ::std::result::Result::Ok(())
}

/// 就寝を記録
#[poise::command(slash_command)]
async fn start(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let now = chrono::Local::now().naive_local();

    let open = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sleep_logs WHERE user_id = ? AND guild_id = ? AND wake_at IS NULL",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_one(&data.db)
    .await?;

    if open > 0 {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("⚠️ 睡眠記録")
                    .description("既に就寝中の記録があります。\n`/sleep end` で起床を記録してください。")
                    .color(0xFEE75C),
            ),
        )
        .await?;
        return ::std::result::Result::Ok(());
    }

    sqlx::query("INSERT INTO sleep_logs (user_id, guild_id, sleep_at) VALUES (?, ?, ?)")
        .bind(ctx.author().id.to_string())
        .bind(&guild_id)
        .bind(now.format("%Y-%m-%d %H:%M:%S").to_string())
        .execute(&data.db)
        .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🌙 おやすみなさい")
                .description(format!("就寝: {}", now.format("%H:%M")))
                .color(0x2C2F33),
        ),
    )
    .await?;
    ::std::result::Result::Ok(())
}

/// 起床を記録
#[poise::command(slash_command)]
async fn end(
    ctx: Context<'_>,
    #[description = "睡眠の質"] quality: Option<SleepQuality>,
    #[description = "メモ"] memo: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let now = chrono::Local::now().naive_local();

    let record = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, sleep_at FROM sleep_logs WHERE user_id = ? AND guild_id = ? AND wake_at IS NULL ORDER BY sleep_at DESC LIMIT 1",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_optional(&data.db)
    .await?;

    let Some((id, sleep_at_str)) = record else {
        ctx.say("就寝記録がありません。`/sleep start` で就寝を記録してください。")
            .await?;
        return ::std::result::Result::Ok(());
    };

    let quality_str = quality.map(|q| q.as_str().to_string());

    sqlx::query("UPDATE sleep_logs SET wake_at = ?, quality = ?, memo = ? WHERE id = ?")
        .bind(now.format("%Y-%m-%d %H:%M:%S").to_string())
        .bind(&quality_str)
        .bind(&memo)
        .bind(id)
        .execute(&data.db)
        .await?;

    let sleep_at = chrono::NaiveDateTime::parse_from_str(&sleep_at_str, "%Y-%m-%d %H:%M:%S")
        .unwrap_or_default();
    let duration = now - sleep_at;
    let hours = duration.num_hours();
    let mins = duration.num_minutes() % 60;

    let goal = get_sleep_goal(&data.db, &ctx.author().id.to_string(), &guild_id).await;
    let goal_str = if let Some(goal_hours) = goal {
        let actual = duration.num_minutes() as f64 / 60.0;
        if actual >= goal_hours {
            format!("🎯 目標 {}h 達成！", goal_hours)
        } else {
            format!("🎯 目標 {}h まであと {:.0}分", goal_hours, (goal_hours - actual) * 60.0)
        }
    } else {
        String::new()
    };

    let mut embed = CreateEmbed::new()
        .title("☀️ おはようございます")
        .color(0xFEE75C)
        .field("就寝", sleep_at.format("%H:%M").to_string(), true)
        .field("起床", now.format("%H:%M").to_string(), true)
        .field("睡眠時間", format!("**{}時間{}分**", hours, mins), true);

    if let Some(q) = quality {
        embed = embed.field("質", format!("{} {}", q.emoji(), q.as_str()), true);
    }
    if let Some(ref m) = memo {
        embed = embed.field("メモ", m, false);
    }
    if !goal_str.is_empty() {
        embed = embed.field("", goal_str, false);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    ::std::result::Result::Ok(())
}

/// 手動で睡眠記録を追加
#[poise::command(slash_command)]
async fn log(
    ctx: Context<'_>,
    #[description = "就寝時刻 (HH:MM or YYYY-MM-DD HH:MM)"] sleep_time: String,
    #[description = "起床時刻 (HH:MM or YYYY-MM-DD HH:MM)"] wake_time: String,
    #[description = "睡眠の質"] quality: Option<SleepQuality>,
    #[description = "メモ"] memo: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let sleep_at = parse_datetime(&sleep_time)?;
    let wake_at = parse_datetime(&wake_time)?;

    if wake_at <= sleep_at {
        ctx.say("起床時刻は就寝時刻より後にしてください。").await?;
        return ::std::result::Result::Ok(());
    }

    let quality_str = quality.map(|q| q.as_str().to_string());

    sqlx::query(
        "INSERT INTO sleep_logs (user_id, guild_id, sleep_at, wake_at, quality, memo) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(sleep_at.format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(wake_at.format("%Y-%m-%d %H:%M:%S").to_string())
    .bind(&quality_str)
    .bind(&memo)
    .execute(&data.db)
    .await?;

    let duration = wake_at - sleep_at;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🌙 睡眠記録追加")
                .color(0x57F287)
                .field("就寝", sleep_at.format("%m/%d %H:%M").to_string(), true)
                .field("起床", wake_at.format("%m/%d %H:%M").to_string(), true)
                .field(
                    "睡眠時間",
                    format!("{}h{}m", duration.num_hours(), duration.num_minutes() % 60),
                    true,
                ),
        ),
    )
    .await?;
    ::std::result::Result::Ok(())
}

/// 睡眠統計
#[poise::command(slash_command)]
async fn stats(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let user_id = ctx.author().id.to_string();

    let week = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT sleep_at, wake_at, quality FROM sleep_logs WHERE user_id = ? AND guild_id = ? AND wake_at IS NOT NULL AND sleep_at >= datetime('now', '-7 days') ORDER BY sleep_at DESC",
    )
    .bind(&user_id)
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    let month = sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT sleep_at, wake_at, quality FROM sleep_logs WHERE user_id = ? AND guild_id = ? AND wake_at IS NOT NULL AND sleep_at >= datetime('now', '-30 days') ORDER BY sleep_at DESC",
    )
    .bind(&user_id)
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if month.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("😴 睡眠統計")
                    .description("まだ記録がありません。\n`/sleep start` で記録を始めましょう！")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return ::std::result::Result::Ok(());
    }

    let week_avg = calc_avg_hours(&week);
    let month_avg = calc_avg_hours(&month);

    let _week_sleep_times: Vec<String> = week
        .iter()
        .map(|(s, _, _)| {
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.format("%H:%M").to_string())
                .unwrap_or_default()
        })
        .collect();

    let avg_sleep_time = if !week.is_empty() {
        let total_mins: i64 = week
            .iter()
            .filter_map(|(s, _, _)| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok()
            })
            .map(|dt| {
                let h = dt.hour() as i64;
                let m = dt.minute() as i64;
                if h < 12 { (h + 24) * 60 + m } else { h * 60 + m }
            })
            .sum();
        let avg = total_mins / week.len() as i64;
        let h = (avg / 60) % 24;
        let m = avg % 60;
        format!("{:02}:{:02}", h, m)
    } else {
        "-".to_string()
    };

    let quality_dist = calc_quality_dist(&month);
    let goal = get_sleep_goal(&data.db, &user_id, &guild_id).await;

    let mut embed = CreateEmbed::new()
        .title("😴 睡眠統計")
        .color(0x5865F2)
        .field(
            "週間平均",
            format!("**{:.1}時間** ({}件)", week_avg, week.len()),
            true,
        )
        .field(
            "月間平均",
            format!("**{:.1}時間** ({}件)", month_avg, month.len()),
            true,
        )
        .field("平均就寝時刻", avg_sleep_time, true);

    if let Some(goal_hours) = goal {
        let achieved = month
            .iter()
            .filter(|(s, w, _)| {
                let dur = calc_duration(s, w);
                dur >= goal_hours
            })
            .count();
        let rate = if month.is_empty() {
            0.0
        } else {
            achieved as f64 / month.len() as f64 * 100.0
        };
        embed = embed.field("🎯 目標達成率", format!("{:.0}% ({}/{}件)", rate, achieved, month.len()), false);
    }

    if !quality_dist.is_empty() {
        embed = embed.field("睡眠の質", quality_dist, false);
    }

    // 直近7日のグラフ
    if !week.is_empty() {
        let graph: String = week
            .iter()
            .rev()
            .map(|(s, w, _)| {
                let hours = calc_duration(s, w);
                let bars = (hours * 2.0).round() as usize;
                let date = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .map(|dt| dt.format("%m/%d").to_string())
                    .unwrap_or_default();
                format!("{} {} {:.1}h", date, "█".repeat(bars.min(16)), hours)
            })
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field("直近7日", format!("```\n{}\n```", graph), false);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    ::std::result::Result::Ok(())
}

/// 目標睡眠時間を設定
#[poise::command(slash_command)]
async fn goal(
    ctx: Context<'_>,
    #[description = "目標睡眠時間（時間、例: 7.5）"] hours: f64,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    sqlx::query(
        "INSERT INTO guild_settings (guild_id, key, value) VALUES (?, ?, ?) ON CONFLICT(guild_id, key) DO UPDATE SET value = excluded.value",
    )
    .bind(&guild_id)
    .bind(format!("sleep_goal_{}", ctx.author().id))
    .bind(hours.to_string())
    .execute(&data.db)
    .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🎯 目標睡眠時間設定")
                .description(format!("目標: **{}時間**", hours))
                .color(0x57F287),
        ),
    )
    .await?;
    ::std::result::Result::Ok(())
}

/// 直近の睡眠記録一覧
#[poise::command(slash_command)]
async fn history(
    ctx: Context<'_>,
    #[description = "件数 (デフォルト7)"] count: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let count = count.unwrap_or(7);

    let logs = sqlx::query_as::<_, (String, Option<String>, Option<String>, Option<String>)>(
        "SELECT sleep_at, wake_at, quality, memo FROM sleep_logs WHERE user_id = ? AND guild_id = ? ORDER BY sleep_at DESC LIMIT ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(count)
    .fetch_all(&data.db)
    .await?;

    if logs.is_empty() {
        ctx.say("睡眠記録がありません。").await?;
        return ::std::result::Result::Ok(());
    }

    let desc: String = logs
        .iter()
        .map(|(sleep_at, wake_at, quality, memo)| {
            let s = chrono::NaiveDateTime::parse_from_str(sleep_at, "%Y-%m-%d %H:%M:%S")
                .map(|dt| dt.format("%m/%d %H:%M").to_string())
                .unwrap_or_else(|_| sleep_at.clone());

            let (w, dur) = if let Some(wake) = wake_at {
                let w_str = chrono::NaiveDateTime::parse_from_str(wake, "%Y-%m-%d %H:%M:%S")
                    .map(|dt| dt.format("%H:%M").to_string())
                    .unwrap_or_else(|_| wake.clone());
                let hours = calc_duration(sleep_at, wake);
                (w_str, format!(" ({:.1}h)", hours))
            } else {
                ("就寝中...".to_string(), String::new())
            };

            let q = quality
                .as_deref()
                .map(|q| format!(" {}", quality_emoji(q)))
                .unwrap_or_default();
            let m = memo
                .as_deref()
                .map(|m| format!(" — {}", m))
                .unwrap_or_default();

            format!("{} → {}{}{}{}", s, w, dur, q, m)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("😴 睡眠記録")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    ::std::result::Result::Ok(())
}

// --- ヘルパー ---

use chrono::Timelike;

fn parse_datetime(input: &str) -> Result<chrono::NaiveDateTime, Error> {
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(input, "%Y-%m-%d %H:%M") {
        return ::std::result::Result::Ok(dt);
    }
    if let Ok(t) = chrono::NaiveTime::parse_from_str(input, "%H:%M") {
        let today = chrono::Local::now().date_naive();
        return ::std::result::Result::Ok(today.and_time(t));
    }
    Err("時刻形式が不正。HH:MM or YYYY-MM-DD HH:MM".into())
}

fn calc_duration(sleep_at: &str, wake_at: &str) -> f64 {
    let s = chrono::NaiveDateTime::parse_from_str(sleep_at, "%Y-%m-%d %H:%M:%S").unwrap_or_default();
    let w = chrono::NaiveDateTime::parse_from_str(wake_at, "%Y-%m-%d %H:%M:%S").unwrap_or_default();
    (w - s).num_minutes() as f64 / 60.0
}

fn calc_avg_hours(records: &[(String, String, Option<String>)]) -> f64 {
    if records.is_empty() {
        return 0.0;
    }
    let total: f64 = records
        .iter()
        .map(|(s, w, _)| calc_duration(s, w))
        .sum();
    total / records.len() as f64
}

fn calc_quality_dist(records: &[(String, String, Option<String>)]) -> String {
    let mut good = 0;
    let mut ok = 0;
    let mut bad = 0;
    for (_, _, q) in records {
        match q.as_deref() {
            Some("good") => good += 1,
            Some("ok") => ok += 1,
            Some("bad") => bad += 1,
            _ => {}
        }
    }
    let total = good + ok + bad;
    if total == 0 {
        return String::new();
    }
    format!("😊{} 😐{} 😢{}", good, ok, bad)
}

async fn get_sleep_goal(
    db: &sqlx::SqlitePool,
    user_id: &str,
    guild_id: &str,
) -> Option<f64> {
    let key = format!("sleep_goal_{}", user_id);
    sqlx::query_scalar::<_, String>(
        "SELECT value FROM guild_settings WHERE guild_id = ? AND key = ?",
    )
    .bind(guild_id)
    .bind(&key)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()
    .and_then(|v| v.parse::<f64>().ok())
}
