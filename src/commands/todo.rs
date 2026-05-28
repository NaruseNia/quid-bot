use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum Priority {
    #[name = "high"]
    High,
    #[name = "medium"]
    Medium,
    #[name = "low"]
    Low,
}

impl Priority {
    fn as_str(self) -> &'static str {
        match self {
            Priority::High => "high",
            Priority::Medium => "medium",
            Priority::Low => "low",
        }
    }

    fn emoji(self) -> &'static str {
        match self {
            Priority::High => "🔴",
            Priority::Medium => "🟡",
            Priority::Low => "🟢",
        }
    }
}

/// TODO管理
#[poise::command(slash_command, subcommands("add", "list", "done", "delete"))]
pub async fn todo(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// タスクを追加
#[poise::command(slash_command)]
async fn add(
    ctx: Context<'_>,
    #[description = "タスク名"] title: String,
    #[description = "優先度"] priority: Option<Priority>,
    #[description = "期限 (YYYY-MM-DD)"] due_date: Option<String>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let priority = priority.unwrap_or(Priority::Medium);

    let parsed_due = due_date
        .as_deref()
        .map(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d"))
        .transpose()
        .map_err(|_| "日付形式が不正。YYYY-MM-DD で指定してください。")?;

    let result = sqlx::query(
        "INSERT INTO todos (user_id, guild_id, title, priority, due_date) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&title)
    .bind(priority.as_str())
    .bind(parsed_due.map(|d| d.to_string()))
    .execute(&data.db)
    .await?;

    let id = result.last_insert_rowid();
    let mut embed = CreateEmbed::new()
        .title("✅ タスク追加")
        .color(0x57F287)
        .field("タスク", format!("#{} {}", id, title), false)
        .field("優先度", format!("{} {}", priority.emoji(), priority.as_str()), true);

    if let Some(ref d) = due_date {
        embed = embed.field("期限", d, true);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// タスク一覧を表示
#[poise::command(slash_command)]
async fn list(
    ctx: Context<'_>,
    #[description = "完了済みも表示"] show_completed: Option<bool>,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let show_completed = show_completed.unwrap_or(false);

    let todos = if show_completed {
        sqlx::query_as::<_, (i64, String, String, Option<String>, bool)>(
            "SELECT id, title, priority, due_date, completed FROM todos WHERE user_id = ? AND guild_id = ? ORDER BY completed ASC, CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, created_at DESC",
        )
    } else {
        sqlx::query_as::<_, (i64, String, String, Option<String>, bool)>(
            "SELECT id, title, priority, due_date, completed FROM todos WHERE user_id = ? AND guild_id = ? AND completed = 0 ORDER BY CASE priority WHEN 'high' THEN 0 WHEN 'medium' THEN 1 ELSE 2 END, created_at DESC",
        )
    }
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if todos.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("📋 TODO一覧")
                    .description("タスクはありません。")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return Ok(());
    }

    let desc: String = todos
        .iter()
        .map(|(id, title, priority, due_date, completed)| {
            let emoji = match priority.as_str() {
                "high" => "🔴",
                "medium" => "🟡",
                _ => "🟢",
            };
            let check = if *completed { "~~" } else { "" };
            let due = due_date
                .as_deref()
                .map(|d| format!(" (期限: {})", d))
                .unwrap_or_default();
            format!("{} **#{}** {}{}{}{}", emoji, id, check, title, check, due)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📋 TODO一覧")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

/// タスクを完了にする
#[poise::command(slash_command)]
async fn done(ctx: Context<'_>, #[description = "タスクID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();
    let result = sqlx::query(
        "UPDATE todos SET completed = 1, completed_at = datetime('now') WHERE id = ? AND user_id = ?",
    )
    .bind(id)
    .bind(ctx.author().id.to_string())
    .execute(&data.db)
    .await?;

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("該当するタスクが見つかりません。")
                .color(0xED4245),
        )).await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("✅ タスク完了")
                    .description(format!("タスク #{} を完了にしました。", id))
                    .color(0x57F287),
            ),
        )
        .await?;
    }
    Ok(())
}

/// タスクを削除する
#[poise::command(slash_command)]
async fn delete(ctx: Context<'_>, #[description = "タスクID"] id: i64) -> Result<(), Error> {
    let data = ctx.data();
    let result = sqlx::query("DELETE FROM todos WHERE id = ? AND user_id = ?")
        .bind(id)
        .bind(ctx.author().id.to_string())
        .execute(&data.db)
        .await?;

    if result.rows_affected() == 0 {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("該当するタスクが見つかりません。")
                .color(0xED4245),
        )).await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🗑️ タスク削除")
                    .description(format!("タスク #{} を削除しました。", id))
                    .color(0xED4245),
            ),
        )
        .await?;
    }
    Ok(())
}

pub async fn get_completed_todos_for_date(
    pool: &sqlx::SqlitePool,
    user_id: &str,
    guild_id: &str,
    date: &str,
) -> Result<Vec<(i64, String, String)>, Error> {
    let todos = sqlx::query_as::<_, (i64, String, String)>(
        "SELECT id, title, priority FROM todos WHERE user_id = ? AND guild_id = ? AND completed = 1 AND date(completed_at) = ?",
    )
    .bind(user_id)
    .bind(guild_id)
    .bind(date)
    .fetch_all(pool)
    .await?;
    Ok(todos)
}
