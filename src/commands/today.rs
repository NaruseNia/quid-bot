use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// デイリーブリーフィング
#[poise::command(
    slash_command,
    subcommands("show", "city", "feeds", "subscribe", "unsubscribe")
)]
pub async fn today(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 今日のブリーフィングを表示
#[poise::command(slash_command, rename = "show")]
async fn show(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let user_id = ctx.author().id.to_string();
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let weekday = weekday_ja(chrono::Local::now().format("%A").to_string());

    let embed = build_briefing(
        &data.http_client,
        &data.db,
        &data.config,
        &guild_id,
        &user_id,
        &date,
        &weekday,
    )
    .await?;

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 天気の都市を設定（サーバー単位）
#[poise::command(slash_command)]
async fn city(
    ctx: Context<'_>,
    #[description = "都市名 (例: Tokyo, Osaka, Nagoya)"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let coords = geocode(&data.http_client, &name).await?;
    let Some((lat, lon, display)) = coords else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description(format!("都市「{}」が見つかりません。英語名で試してみてください。", name))
                .color(0xED4245),
        )).await?;
        return Ok(());
    };

    set_guild_setting(&data.db, &guild_id, "weather_city", &display).await?;
    set_guild_setting(&data.db, &guild_id, "weather_lat", &lat.to_string()).await?;
    set_guild_setting(&data.db, &guild_id, "weather_lon", &lon.to_string()).await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🌤 天気都市設定")
                .description(format!("天気の対象都市を **{}** に設定しました。", display))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// ブリーフィングのニュースカテゴリを設定（サーバー単位）
#[poise::command(slash_command)]
async fn feeds(
    ctx: Context<'_>,
    #[description = "カテゴリ (カンマ区切り: tech,japan,world,business)"] categories: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let cats: Vec<&str> = categories.split(',').map(|s| s.trim()).collect();
    let invalid: Vec<&&str> = cats
        .iter()
        .filter(|c| !super::news::CATEGORIES.contains(c))
        .collect();

    if !invalid.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description(format!(
                    "無効なカテゴリ: {}。使用可能: {}",
                    invalid.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(", "),
                    super::news::CATEGORIES.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(", "),
                ))
                .color(0xED4245),
        )).await?;
        return Ok(());
    }

    set_guild_setting(&data.db, &guild_id, "today_feeds", &categories).await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📰 ニュースカテゴリ設定")
                .description(format!(
                    "ブリーフィングのニュースカテゴリ: {}",
                    cats.iter().map(|c| format!("`{}`", c)).collect::<Vec<_>>().join(" "),
                ))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// 毎日の自動投稿を設定（管理者のみ）
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD")]
async fn subscribe(
    ctx: Context<'_>,
    #[description = "投稿時刻 (HH:MM)"] time: String,
    #[description = "投稿チャンネル（省略で現在のチャンネル）"] channel: Option<serenity::ChannelId>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let channel_id = channel.unwrap_or(ctx.channel_id());

    sqlx::query(
        "INSERT INTO today_subscriptions (guild_id, channel_id, post_time) VALUES (?, ?, ?) ON CONFLICT(guild_id) DO UPDATE SET channel_id = excluded.channel_id, post_time = excluded.post_time, is_active = 1",
    )
    .bind(&guild_id)
    .bind(channel_id.to_string())
    .bind(&time)
    .execute(&data.db)
    .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📅 自動ブリーフィング設定")
                .description(format!(
                    "毎日 **{}** に <#{}> へ投稿します。",
                    time, channel_id
                ))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// 自動投稿を解除（管理者のみ）
#[poise::command(slash_command, required_permissions = "MANAGE_GUILD")]
async fn unsubscribe(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    sqlx::query("UPDATE today_subscriptions SET is_active = 0 WHERE guild_id = ?")
        .bind(&guild_id)
        .execute(&data.db)
        .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📅 自動ブリーフィング解除")
                .description("毎日の自動投稿を停止しました。")
                .color(0xED4245),
        ),
    )
    .await?;
    Ok(())
}

// --- 公開: バックグラウンドループ用 ---

pub async fn today_loop(
    http: std::sync::Arc<serenity::Http>,
    pool: sqlx::SqlitePool,
    http_client: reqwest::Client,
    config: crate::config::Config,
) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(60)).await;

        let now = chrono::Local::now();
        let time_str = now.format("%H:%M").to_string();

        let subs = sqlx::query_as::<_, (String, String)>(
            "SELECT guild_id, channel_id FROM today_subscriptions WHERE is_active = 1 AND post_time = ?",
        )
        .bind(&time_str)
        .fetch_all(&pool)
        .await;

        let Ok(subs) = subs else { continue };

        // 同じ分に複数回実行しないよう、秒が0-59の間のみ（60秒スリープなので1回だけ発火）
        for (guild_id, channel_id_str) in subs {
            let channel_id: serenity::ChannelId =
                channel_id_str.parse::<u64>().unwrap_or(0).into();

            let date = now.format("%Y-%m-%d").to_string();
            let weekday = weekday_ja(now.format("%A").to_string());

            // 自動投稿ではuser固有情報（TODO等）は含めない
            let embed = match build_briefing(
                &http_client,
                &pool,
                &config,
                &guild_id,
                "",
                &date,
                &weekday,
            )
            .await
            {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("today auto-post failed for guild {}: {}", guild_id, e);
                    continue;
                }
            };

            channel_id
                .send_message(&http, serenity::CreateMessage::new().embed(embed))
                .await
                .ok();
        }
    }
}

// --- 内部ヘルパー ---

async fn build_briefing(
    http_client: &reqwest::Client,
    db: &sqlx::SqlitePool,
    config: &crate::config::Config,
    guild_id: &str,
    user_id: &str,
    date: &str,
    weekday: &str,
) -> Result<CreateEmbed, Error> {
    let mut embed = CreateEmbed::new()
        .title(format!("☀️ 今日のブリーフィング — {} ({})", date, weekday))
        .color(0xFEE75C);

    // --- 天気 ---
    let city = get_guild_setting(db, guild_id, "weather_city").await;
    let lat = get_guild_setting(db, guild_id, "weather_lat").await;
    let lon = get_guild_setting(db, guild_id, "weather_lon").await;

    if let (Some(city), Some(lat), Some(lon)) = (city, lat, lon) {
        let weather = fetch_weather(http_client, &lat, &lon).await;
        match weather {
            Ok(w) => {
                embed = embed.field(
                    format!("🌤 天気 — {}", city),
                    format!(
                        "{} **{}℃** / {}℃　降水確率 {}%",
                        w.icon, w.temp_max, w.temp_min, w.precipitation_prob
                    ),
                    false,
                );
            }
            Err(e) => {
                tracing::warn!("weather fetch failed: {}", e);
                embed = embed.field("🌤 天気", "取得に失敗しました", false);
            }
        }
    } else {
        embed = embed.field(
            "🌤 天気",
            "都市が未設定です。`/today city Tokyo` で設定してください。",
            false,
        );
    }

    // --- ニュース ---
    let feed_cats = get_guild_setting(db, guild_id, "today_feeds")
        .await
        .unwrap_or_else(|| "japan".to_string());

    let mut all_entries = Vec::new();
    for cat in feed_cats.split(',').map(|s| s.trim()) {
        let urls = super::news::get_feed_urls(db, guild_id, cat).await.unwrap_or_default();
        let entries = super::news::fetch_feeds(http_client, &urls, 3).await;
        all_entries.extend(entries);
    }

    if !all_entries.is_empty() {
        let titles: String = all_entries
            .iter()
            .map(|e| format!("- {}", e.title))
            .collect::<Vec<_>>()
            .join("\n");

        let summary =
            super::news::generate_news_summary(http_client, db, guild_id, config, &titles).await;
        embed = embed.field("📰 ニュース", summary, false);
    }

    // --- ユーザー固有情報 ---
    if !user_id.is_empty() {
        // TODO
        let todos = sqlx::query_as::<_, (i64, String, String, Option<String>)>(
            "SELECT id, title, priority, due_date FROM todos WHERE user_id = ? AND guild_id = ? AND completed = 0 ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END LIMIT 5",
        )
        .bind(user_id)
        .bind(guild_id)
        .fetch_all(db)
        .await
        .unwrap_or_default();

        if !todos.is_empty() {
            let todo_str: String = todos
                .iter()
                .map(|(id, title, priority, due)| {
                    let emoji = match priority.as_str() {
                        "high" => "🔴",
                        "medium" => "🟡",
                        _ => "🟢",
                    };
                    let due_str = due
                        .as_deref()
                        .map(|d| {
                            if d == chrono::Local::now().format("%Y-%m-%d").to_string() {
                                " ⚠️**今日**".to_string()
                            } else {
                                format!(" ({})", d)
                            }
                        })
                        .unwrap_or_default();
                    format!("{} #{} {}{}", emoji, id, title, due_str)
                })
                .collect::<Vec<_>>()
                .join("\n");
            embed = embed.field("📋 TODO", todo_str, false);
        }

        // 習慣
        let today_date = chrono::Local::now().format("%Y-%m-%d").to_string();
        let habits = sqlx::query_as::<_, (String, i64)>(
            "SELECT h.name, COALESCE((SELECT COUNT(*) FROM habit_logs WHERE habit_id = h.id AND checked_at = ?), 0) FROM habits h WHERE h.user_id = ? AND h.guild_id = ? AND h.is_active = 1",
        )
        .bind(&today_date)
        .bind(user_id)
        .bind(guild_id)
        .fetch_all(db)
        .await
        .unwrap_or_default();

        if !habits.is_empty() {
            let habit_str: String = habits
                .iter()
                .map(|(name, checked)| {
                    if *checked > 0 {
                        format!("✅ {}", name)
                    } else {
                        format!("⬜ {}", name)
                    }
                })
                .collect::<Vec<_>>()
                .join("　");
            embed = embed.field("🎯 習慣", habit_str, false);
        }

        // リマインダー
        let reminders = sqlx::query_as::<_, (String, String)>(
            "SELECT remind_at, message FROM reminders WHERE user_id = ? AND guild_id = ? AND is_active = 1 AND date(remind_at) = ? ORDER BY remind_at",
        )
        .bind(user_id)
        .bind(guild_id)
        .bind(&today_date)
        .fetch_all(db)
        .await
        .unwrap_or_default();

        if !reminders.is_empty() {
            let remind_str: String = reminders
                .iter()
                .map(|(time, msg)| {
                    let t = chrono::NaiveDateTime::parse_from_str(time, "%Y-%m-%d %H:%M:%S")
                        .map(|dt| dt.format("%H:%M").to_string())
                        .unwrap_or_else(|_| time.clone());
                    format!("⏰ {} — {}", t, msg)
                })
                .collect::<Vec<_>>()
                .join("\n");
            embed = embed.field("⏰ リマインダー", remind_str, false);
        }
    }

    Ok(embed)
}

struct WeatherInfo {
    temp_max: f64,
    temp_min: f64,
    precipitation_prob: f64,
    icon: String,
}

async fn fetch_weather(
    http_client: &reqwest::Client,
    lat: &str,
    lon: &str,
) -> Result<WeatherInfo, Error> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={}&longitude={}&daily=temperature_2m_max,temperature_2m_min,precipitation_probability_max,weather_code&timezone=Asia/Tokyo&forecast_days=1",
        lat, lon
    );

    let resp: serde_json::Value = http_client.get(&url).send().await?.json().await?;

    let daily = &resp["daily"];
    let temp_max = daily["temperature_2m_max"][0].as_f64().unwrap_or(0.0);
    let temp_min = daily["temperature_2m_min"][0].as_f64().unwrap_or(0.0);
    let precip = daily["precipitation_probability_max"][0].as_f64().unwrap_or(0.0);
    let code = daily["weather_code"][0].as_i64().unwrap_or(0);

    let icon = weather_icon(code);

    Ok(WeatherInfo {
        temp_max,
        temp_min,
        precipitation_prob: precip,
        icon,
    })
}

async fn geocode(
    http_client: &reqwest::Client,
    city: &str,
) -> Result<Option<(f64, f64, String)>, Error> {
    let url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=1&language=ja",
        city
    );
    let resp: serde_json::Value = http_client.get(&url).send().await?.json().await?;

    let results = resp["results"].as_array();
    let Some(results) = results else {
        return Ok(None);
    };
    let Some(first) = results.first() else {
        return Ok(None);
    };

    let lat = first["latitude"].as_f64().unwrap_or(0.0);
    let lon = first["longitude"].as_f64().unwrap_or(0.0);
    let name = first["name"].as_str().unwrap_or(city).to_string();
    let country = first["country"].as_str().unwrap_or("");
    let display = if country.is_empty() {
        name
    } else {
        format!("{}, {}", name, country)
    };

    Ok(Some((lat, lon, display)))
}

async fn set_guild_setting(
    db: &sqlx::SqlitePool,
    guild_id: &str,
    key: &str,
    value: &str,
) -> Result<(), Error> {
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

async fn get_guild_setting(
    db: &sqlx::SqlitePool,
    guild_id: &str,
    key: &str,
) -> Option<String> {
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

fn weather_icon(code: i64) -> String {
    match code {
        0 => "☀️",
        1..=3 => "⛅",
        45 | 48 => "🌫️",
        51..=57 => "🌦️",
        61..=67 => "🌧️",
        71..=77 => "🌨️",
        80..=82 => "🌧️",
        85 | 86 => "🌨️",
        95..=99 => "⛈️",
        _ => "🌤",
    }
    .to_string()
}

fn weekday_ja(en: String) -> String {
    match en.as_str() {
        "Monday" => "月",
        "Tuesday" => "火",
        "Wednesday" => "水",
        "Thursday" => "木",
        "Friday" => "金",
        "Saturday" => "土",
        "Sunday" => "日",
        _ => &en,
    }
    .to_string()
}
