use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// 日記
#[poise::command(slash_command, subcommands("write", "list", "view", "search"))]
pub async fn diary(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 日記を書く
#[poise::command(slash_command)]
async fn write(
    ctx: Context<'_>,
    #[description = "日記の内容"] content: String,
    #[description = "気分 (😊😐😢🔥😴 等)"] mood: Option<String>,
    #[description = "タグ (カンマ区切り)"] tags: Option<String>,
    #[description = "公開する (デフォルト: 公開)"] public: Option<bool>,
    #[description = "日付 (YYYY-MM-DD、省略で今日)"] date: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let is_public = public.unwrap_or(true);
    let date = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    let entry = serde_json::json!({
        "content": content,
        "mood": mood,
        "tags": tags.as_deref().map(|t| t.split(',').map(|s| s.trim()).collect::<Vec<_>>()),
    });

    save_diary(
        &data.db,
        &ctx.author().id.to_string(),
        &guild_id,
        &entry.to_string(),
        is_public,
        &date,
    )
    .await?;

    let completed = super::todo::get_completed_todos_for_date(
        &data.db,
        &ctx.author().id.to_string(),
        &guild_id,
        &date,
    )
    .await?;

    let mut embed = CreateEmbed::new()
        .title(format!("📔 {}の日記", date))
        .description(&content)
        .color(0x57F287)
        .timestamp(serenity::Timestamp::now());

    if let Some(ref m) = mood {
        embed = embed.field("気分", m, true);
    }

    if let Some(ref t) = tags {
        let tag_str: String = t
            .split(',')
            .map(|s| format!("`{}`", s.trim()))
            .collect::<Vec<_>>()
            .join(" ");
        embed = embed.field("タグ", tag_str, true);
    }

    if !completed.is_empty() {
        let tasks: String = completed
            .iter()
            .map(|(id, title, priority)| {
                let emoji = priority_emoji(priority);
                format!("{} #{} {}", emoji, id, title)
            })
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field("✅ 今日の完了タスク", tasks, false);
    }

    if !is_public {
        embed = embed.footer(serenity::CreateEmbedFooter::new("🔒 非公開"));
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 過去の日記一覧
#[poise::command(slash_command)]
async fn list(
    ctx: Context<'_>,
    #[description = "件数 (デフォルト10)"] limit: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let limit = limit.unwrap_or(10);

    let diaries = sqlx::query_as::<_, (i64, String, bool, String)>(
        "SELECT id, date, is_public, content FROM diaries WHERE user_id = ? AND guild_id = ? ORDER BY date DESC LIMIT ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(limit)
    .fetch_all(&data.db)
    .await?;

    if diaries.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("📔 日記一覧")
                    .description("日記はまだありません。\n`/diary write` で書き始めましょう！")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return Ok(());
    }

    let desc: String = diaries
        .iter()
        .map(|(id, date, is_public, content)| {
            let vis = if *is_public { "" } else { " 🔒" };
            let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or_default();
            let mood = parsed.get("mood").and_then(|m| m.as_str()).unwrap_or("");
            let preview = parsed
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .chars()
                .take(30)
                .collect::<String>();
            let ellipsis = if preview.len() >= 30 { "..." } else { "" };
            format!(
                "**#{}** | {} {} {}{}{}", id, date, mood, preview, ellipsis, vis
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📔 日記一覧")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

/// 特定日の日記を表示
#[poise::command(slash_command)]
async fn view(
    ctx: Context<'_>,
    #[description = "日付 (YYYY-MM-DD)"] date: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let diary = sqlx::query_as::<_, (String, bool)>(
        "SELECT content, is_public FROM diaries WHERE user_id = ? AND guild_id = ? AND date = ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&date)
    .fetch_optional(&data.db)
    .await?;

    let Some((content, is_public)) = diary else {
        ctx.say(format!("{} の日記はありません。", date)).await?;
        return Ok(());
    };

    let parsed: serde_json::Value = serde_json::from_str(&content)?;

    let body = parsed
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("");

    let mut embed = CreateEmbed::new()
        .title(format!("📔 {}の日記", date))
        .description(body)
        .color(0x57F287);

    if let Some(mood) = parsed.get("mood").and_then(|m| m.as_str())
        && !mood.is_empty()
    {
        embed = embed.field("気分", mood, true);
    }

    if let Some(tags) = parsed.get("tags").and_then(|t| t.as_array()) {
        let tag_str: String = tags
            .iter()
            .filter_map(|t| t.as_str())
            .map(|t| format!("`{}`", t))
            .collect::<Vec<_>>()
            .join(" ");
        if !tag_str.is_empty() {
            embed = embed.field("タグ", tag_str, true);
        }
    }

    if !is_public {
        embed = embed.footer(serenity::CreateEmbedFooter::new("🔒 非公開"));
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// タグで日記を検索
#[poise::command(slash_command)]
async fn search(
    ctx: Context<'_>,
    #[description = "検索キーワード"] keyword: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let diaries = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, date, content FROM diaries WHERE user_id = ? AND guild_id = ? AND content LIKE ? ORDER BY date DESC LIMIT 10",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(format!("%{}%", keyword))
    .fetch_all(&data.db)
    .await?;

    if diaries.is_empty() {
        ctx.say(format!("「{}」に一致する日記はありません。", keyword))
            .await?;
        return Ok(());
    }

    let desc: String = diaries
        .iter()
        .map(|(id, date, content)| {
            let parsed: serde_json::Value = serde_json::from_str(content).unwrap_or_default();
            let preview = parsed
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .chars()
                .take(40)
                .collect::<String>();
            format!("**#{}** | {} — {}...", id, date, preview)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title(format!("🔍 「{}」の検索結果", keyword))
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

async fn save_diary(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    guild_id: &str,
    content: &str,
    is_public: bool,
    date: &str,
) -> Result<(), Error> {
    sqlx::query(
        "INSERT INTO diaries (user_id, guild_id, content, is_public, date) VALUES (?, ?, ?, ?, ?) ON CONFLICT(user_id, guild_id, date) DO UPDATE SET content = excluded.content, is_public = excluded.is_public",
    )
    .bind(user_id)
    .bind(guild_id)
    .bind(content)
    .bind(is_public)
    .bind(date)
    .execute(pool)
    .await?;
    Ok(())
}

fn priority_emoji(priority: &str) -> &'static str {
    match priority {
        "high" => "🔴",
        "medium" => "🟡",
        _ => "🟢",
    }
}
