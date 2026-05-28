use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;

/// メモ/スニペット管理
#[poise::command(
    slash_command,
    subcommands("save", "get", "list_memos", "search", "delete")
)]
pub async fn memo(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// メモを保存
#[poise::command(slash_command)]
async fn save(
    ctx: Context<'_>,
    #[description = "タイトル"] title: String,
    #[description = "内容（コード、URL、テキスト等）"] content: String,
    #[description = "タグ (カンマ区切り)"] tags: Option<String>,
    #[description = "コードの言語 (rust, js, py 等)"] language: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let result = sqlx::query(
        "INSERT INTO memos (user_id, guild_id, title, content, language, tags) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&title)
    .bind(&content)
    .bind(&language)
    .bind(&tags)
    .execute(&data.db)
    .await?;

    let id = result.last_insert_rowid();
    let mut embed = CreateEmbed::new()
        .title(format!("📌 メモ保存 #{}", id))
        .color(0x57F287)
        .field("タイトル", &title, false);

    if let Some(ref lang) = language {
        embed = embed.field("内容", format!("```{}\n{}\n```", lang, &content), false);
    } else if content.len() > 200 {
        embed = embed.field("内容", format!("{}...", &content[..200]), false);
    } else {
        embed = embed.field("内容", &content, false);
    }

    if let Some(ref t) = tags {
        let tag_str: String = t.split(',').map(|s| format!("`{}`", s.trim())).collect::<Vec<_>>().join(" ");
        embed = embed.field("タグ", tag_str, false);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// メモをIDで取得
#[poise::command(slash_command)]
async fn get(ctx: Context<'_>, #[description = "メモID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();

    let memo = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
        "SELECT title, content, language, tags, created_at FROM memos WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(ctx.author().id.to_string())
    .fetch_optional(&data.db)
    .await?;

    let Some((title, content, language, tags, created_at)) = memo else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().description("メモが見つかりません。").color(0xED4245),
        )).await?;
        return Ok(());
    };

    let mut embed = CreateEmbed::new()
        .title(format!("📌 #{} {}", id, title))
        .color(0x5865F2);

    if let Some(ref lang) = language {
        embed = embed.description(format!("```{}\n{}\n```", lang, content));
    } else {
        embed = embed.description(&content);
    }

    if let Some(ref t) = tags {
        let tag_str: String = t.split(',').map(|s| format!("`{}`", s.trim())).collect::<Vec<_>>().join(" ");
        embed = embed.field("タグ", tag_str, true);
    }

    embed = embed.footer(poise::serenity_prelude::CreateEmbedFooter::new(created_at));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// メモ一覧
#[poise::command(slash_command, rename = "list")]
async fn list_memos(
    ctx: Context<'_>,
    #[description = "件数 (デフォルト10)"] limit: Option<i64>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let limit = limit.unwrap_or(10);

    let memos = sqlx::query_as::<_, (i64, String, Option<String>, String)>(
        "SELECT id, title, tags, created_at FROM memos WHERE user_id = ? AND guild_id = ? ORDER BY created_at DESC LIMIT ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(limit)
    .fetch_all(&data.db)
    .await?;

    if memos.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📌 メモ一覧")
                .description("メモはまだありません。\n`/memo save` で保存しましょう！")
                .color(0x99AAB5),
        )).await?;
        return Ok(());
    }

    let desc: String = memos
        .iter()
        .map(|(id, title, tags, date)| {
            let tag_str = tags
                .as_deref()
                .map(|t| {
                    t.split(',')
                        .map(|s| format!("`{}`", s.trim()))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            let date_short = &date[..10.min(date.len())];
            format!("**#{}** {} — {} {}", id, title, date_short, tag_str)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new().title("📌 メモ一覧").description(desc).color(0x5865F2),
    )).await?;
    Ok(())
}

/// タグやキーワードでメモを検索
#[poise::command(slash_command)]
async fn search(
    ctx: Context<'_>,
    #[description = "検索キーワード（タイトル・内容・タグ）"] keyword: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let pattern = format!("%{}%", keyword);

    let memos = sqlx::query_as::<_, (i64, String, Option<String>)>(
        "SELECT id, title, tags FROM memos WHERE user_id = ? AND guild_id = ? AND (title LIKE ? OR content LIKE ? OR tags LIKE ?) ORDER BY created_at DESC LIMIT 10",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&pattern)
    .bind(&pattern)
    .bind(&pattern)
    .fetch_all(&data.db)
    .await?;

    if memos.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description(format!("「{}」に一致するメモはありません。", keyword))
                .color(0xED4245),
        )).await?;
        return Ok(());
    }

    let desc: String = memos
        .iter()
        .map(|(id, title, tags)| {
            let tag_str = tags
                .as_deref()
                .map(|t| t.split(',').map(|s| format!("`{}`", s.trim())).collect::<Vec<_>>().join(" "))
                .unwrap_or_default();
            format!("**#{}** {} {}", id, title, tag_str)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title(format!("🔍 メモ検索「{}」", keyword))
            .description(desc)
            .color(0x5865F2),
    )).await?;
    Ok(())
}

/// メモを削除
#[poise::command(slash_command)]
async fn delete(ctx: Context<'_>, #[description = "メモID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();
    let result = sqlx::query("DELETE FROM memos WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(ctx.author().id.to_string())
        .execute(&data.db)
        .await?;

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().description("メモが見つかりません。").color(0xED4245),
        )).await?;
    } else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🗑️ メモ削除")
                .description(format!("メモ #{} を削除しました。", id))
                .color(0xED4245),
        )).await?;
    }
    Ok(())
}
