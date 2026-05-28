use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

struct DiarySession {
    channel_id: serenity::ChannelId,
    messages: Vec<String>,
    started_at: chrono::NaiveDateTime,
}

type Sessions = Arc<Mutex<HashMap<String, DiarySession>>>;

static ACTIVE_SESSIONS: std::sync::LazyLock<Sessions> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(HashMap::new())));

fn session_key(user_id: serenity::UserId, guild_id: &str) -> String {
    format!("{}_{}", user_id, guild_id)
}

/// 日記
#[poise::command(
    slash_command,
    subcommands("write", "edit", "start", "end", "list", "view", "search", "delete")
)]
pub async fn diary(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 日記を書く（一括）
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

    let embed = build_diary_embed(&data.db, &ctx, &content, &mood, &tags, is_public, &date).await?;
    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 日記モード開始 — 以降の発言を自動収集
#[poise::command(slash_command)]
async fn start(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let key = session_key(ctx.author().id, &guild_id);

    let mut sessions = ACTIVE_SESSIONS.lock().await;
    if sessions.contains_key(&key) {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("⚠️ 日記モード")
                    .description("既に日記モード中です。\n`/diary end` で保存できます。")
                    .color(0xFEE75C),
            ),
        )
        .await?;
        return Ok(());
    }

    sessions.insert(
        key,
        DiarySession {
            channel_id: ctx.channel_id(),
            messages: Vec::new(),
            started_at: chrono::Local::now().naive_local(),
        },
    );

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📔 日記モード開始")
                .description(
                    "このチャンネルでの発言を自動で記録します。\n\
                     `/diary end` で記録を終了して日記として保存します。",
                )
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// 日記モード終了 — 収集した発言を日記として保存
#[poise::command(slash_command)]
async fn end(
    ctx: Context<'_>,
    #[description = "気分 (😊😐😢🔥😴 等)"] mood: Option<String>,
    #[description = "タグ (カンマ区切り)"] tags: Option<String>,
    #[description = "公開する (デフォルト: 公開)"] public: Option<bool>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let key = session_key(ctx.author().id, &guild_id);

    let session = {
        let mut sessions = ACTIVE_SESSIONS.lock().await;
        sessions.remove(&key)
    };

    let Some(session) = session else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("⚠️ 日記モード")
                    .description("日記モードは開始されていません。\n`/diary start` で開始してください。")
                    .color(0xFEE75C),
            ),
        )
        .await?;
        return Ok(());
    };

    if session.messages.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("📔 日記モード終了")
                    .description("記録された発言がありませんでした。")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return Ok(());
    }

    let data = ctx.data();
    let is_public = public.unwrap_or(true);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let content = session.messages.join("\n");

    let entry = serde_json::json!({
        "content": content,
        "mood": mood,
        "tags": tags.as_deref().map(|t| t.split(',').map(|s| s.trim()).collect::<Vec<_>>()),
        "collected": true,
        "message_count": session.messages.len(),
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

    let duration = chrono::Local::now().naive_local() - session.started_at;
    let duration_str = if duration.num_hours() > 0 {
        format!("{}時間{}分", duration.num_hours(), duration.num_minutes() % 60)
    } else {
        format!("{}分", duration.num_minutes())
    };

    let embed =
        build_diary_embed(&data.db, &ctx, &content, &mood, &tags, is_public, &date).await?;
    let embed = embed
        .field(
            "📊 記録情報",
            format!("{}件の発言 / {}", session.messages.len(), duration_str),
            false,
        );

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
                    .description("日記はまだありません。\n`/diary write` か `/diary start` で書き始めましょう！")
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

/// キーワードで日記を検索
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

/// 日記を編集（上書き）
#[poise::command(slash_command)]
async fn edit(
    ctx: Context<'_>,
    #[description = "日付 (YYYY-MM-DD、省略で今日)"] date: Option<String>,
    #[description = "新しい内容"] content: String,
    #[description = "気分"] mood: Option<String>,
    #[description = "タグ (カンマ区切り)"] tags: Option<String>,
    #[description = "公開する"] public: Option<bool>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let date = date.unwrap_or_else(|| chrono::Local::now().format("%Y-%m-%d").to_string());

    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM diaries WHERE user_id = ? AND guild_id = ? AND date = ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&date)
    .fetch_one(&data.db)
    .await?;

    if existing == 0 {
        ctx.say(format!("{} の日記はありません。`/diary write` で作成してください。", date))
            .await?;
        return Ok(());
    }

    let is_public = public.unwrap_or(true);
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

    let embed = build_diary_embed(&data.db, &ctx, &content, &mood, &tags, is_public, &date).await?;
    let embed = embed.field("✏️", "編集済み", true);

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 日記を削除
#[poise::command(slash_command)]
async fn delete(
    ctx: Context<'_>,
    #[description = "日付 (YYYY-MM-DD)"] date: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let result = sqlx::query(
        "DELETE FROM diaries WHERE user_id = ? AND guild_id = ? AND date = ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&date)
    .execute(&data.db)
    .await?;

    if result.rows_affected() == 0 {
        ctx.say(format!("{} の日記はありません。", date)).await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🗑️ 日記削除")
                    .description(format!("{} の日記を削除しました。", date))
                    .color(0xED4245),
            ),
        )
        .await?;
    }
    Ok(())
}

pub async fn handle_message(user_id: serenity::UserId, channel_id: serenity::ChannelId, guild_id: Option<serenity::GuildId>, content: &str) {
    let guild_str = guild_id.map(|g| g.to_string()).unwrap_or_default();
    let key = session_key(user_id, &guild_str);

    let mut sessions = ACTIVE_SESSIONS.lock().await;
    if let Some(session) = sessions.get_mut(&key)
        && session.channel_id == channel_id
    {
        session.messages.push(content.to_string());
    }
}

async fn build_diary_embed(
    pool: &sqlx::SqlitePool,
    ctx: &Context<'_>,
    content: &str,
    mood: &Option<String>,
    tags: &Option<String>,
    is_public: bool,
    date: &str,
) -> Result<CreateEmbed, Error> {
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let completed = super::todo::get_completed_todos_for_date(
        pool,
        &ctx.author().id.to_string(),
        &guild_id,
        date,
    )
    .await?;

    let display = if content.len() > 4000 {
        format!("{}...", &content[..4000])
    } else {
        content.to_string()
    };

    let mut embed = CreateEmbed::new()
        .title(format!("📔 {}の日記", date))
        .description(display)
        .color(0x57F287)
        .timestamp(serenity::Timestamp::now());

    if let Some(m) = mood {
        embed = embed.field("気分", m, true);
    }

    if let Some(t) = tags {
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

    Ok(embed)
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
