use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// スレッドのブックマーク
#[poise::command(slash_command, subcommands("save", "list_bookmarks", "delete"))]
pub async fn thread(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 現在のスレッドをブックマーク
#[poise::command(slash_command)]
async fn save(
    ctx: Context<'_>,
    #[description = "ブックマーク名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let channel_id = ctx.channel_id();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    if let Ok(channel) = channel_id.to_channel(ctx.serenity_context()).await {
        let is_thread = matches!(
            &channel,
            serenity::Channel::Guild(gc) if gc.kind == serenity::ChannelType::PublicThread
                || gc.kind == serenity::ChannelType::PrivateThread
        );
        if !is_thread {
            ctx.send(poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .description("このコマンドはスレッド内でのみ使用できます。")
                    .color(0xED4245),
            )).await?;
            return Ok(());
        }
    }

    sqlx::query(
        "INSERT INTO thread_bookmarks (user_id, guild_id, channel_id, name) VALUES (?, ?, ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(channel_id.to_string())
    .bind(&name)
    .execute(&data.db)
    .await?;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("🔖 スレッドをブックマーク")
            .description(format!("**{}** — <#{}>", name, channel_id))
            .color(0x57F287),
    )).await?;
    Ok(())
}

/// ブックマーク一覧
#[poise::command(slash_command, rename = "list")]
async fn list_bookmarks(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let bookmarks = sqlx::query_as::<_, (i64, String, String, String)>(
        "SELECT id, channel_id, name, created_at FROM thread_bookmarks WHERE user_id = ? AND guild_id = ? ORDER BY created_at DESC",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if bookmarks.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🔖 ブックマーク")
                .description("ブックマークはまだありません。\nスレッド内で `/thread save <名前>` で保存できます。")
                .color(0x99AAB5),
        )).await?;
        return Ok(());
    }

    let desc: String = bookmarks
        .iter()
        .map(|(id, channel_id, name, date)| {
            let date_short = &date[..10.min(date.len())];
            format!("**#{}** {} — <#{}> ({})", id, name, channel_id, date_short)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("🔖 ブックマーク")
            .description(desc)
            .color(0x5865F2),
    )).await?;
    Ok(())
}

/// ブックマークを削除
#[poise::command(slash_command)]
async fn delete(ctx: Context<'_>, #[description = "ブックマークID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();
    let result = sqlx::query("DELETE FROM thread_bookmarks WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(ctx.author().id.to_string())
        .execute(&data.db)
        .await?;

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().description("ブックマークが見つかりません。").color(0xED4245),
        )).await?;
    } else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🗑️ ブックマーク削除")
                .description(format!("ブックマーク #{} を削除しました。", id))
                .color(0xED4245),
        )).await?;
    }
    Ok(())
}
