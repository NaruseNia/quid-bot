use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};

/// 習慣トラッカー
#[poise::command(slash_command, subcommands("add", "check", "list_habits", "stats", "remove"))]
pub async fn habit(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 習慣を登録
#[poise::command(slash_command)]
async fn add(
    ctx: Context<'_>,
    #[description = "習慣名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let result = sqlx::query(
        "INSERT INTO habits (user_id, guild_id, name) VALUES (?, ?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&name)
    .execute(&data.db)
    .await?;

    if result.rows_affected() == 0 {
        ctx.say(format!("「{}」は既に登録されています。", name))
            .await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("✨ 習慣登録")
                    .description(format!("「{}」を登録しました！", name))
                    .color(0x57F287),
            ),
        )
        .await?;
    }
    Ok(())
}

/// 習慣をチェック
#[poise::command(slash_command)]
async fn check(
    ctx: Context<'_>,
    #[description = "習慣名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let habit = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM habits WHERE user_id = ? AND guild_id = ? AND name = ? AND is_active = 1",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&name)
    .fetch_optional(&data.db)
    .await?;

    let Some((habit_id,)) = habit else {
        ctx.say(format!("習慣「{}」が見つかりません。", name))
            .await?;
        return Ok(());
    };

    let result = sqlx::query(
        "INSERT INTO habit_logs (habit_id, checked_at) VALUES (?, ?) ON CONFLICT DO NOTHING",
    )
    .bind(habit_id)
    .bind(&today)
    .execute(&data.db)
    .await?;

    if result.rows_affected() == 0 {
        ctx.say(format!("「{}」は今日既にチェック済みです。", name))
            .await?;
    } else {
        let streak = get_streak(&data.db, habit_id).await?;
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("✅ 習慣達成！")
                    .description(format!("「{}」\n🔥 **{}日連続**", name, streak))
                    .color(0x57F287),
            ),
        )
        .await?;
    }
    Ok(())
}

/// 習慣一覧（今日のチェック状況付き）
#[poise::command(slash_command, rename = "list")]
async fn list_habits(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let habits = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, name FROM habits WHERE user_id = ? AND guild_id = ? AND is_active = 1 ORDER BY name",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if habits.is_empty() {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🎯 習慣一覧")
                    .description("登録されている習慣はありません。\n`/habit add` で追加しましょう！")
                    .color(0x99AAB5),
            ),
        )
        .await?;
        return Ok(());
    }

    let mut desc = String::new();
    let mut buttons = Vec::new();

    for (id, name) in &habits {
        let checked = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM habit_logs WHERE habit_id = ? AND checked_at = ?",
        )
        .bind(id)
        .bind(&today)
        .fetch_one(&data.db)
        .await?;

        let streak = get_streak(&data.db, *id).await?;
        let icon = if checked > 0 { "✅" } else { "⬜" };
        desc.push_str(&format!("{} {} (🔥 {}日)\n", icon, name, streak));

        if checked == 0 {
            buttons.push(
                serenity::CreateButton::new(format!("habit_check_{}", id))
                    .label(name)
                    .style(serenity::ButtonStyle::Success),
            );
        }
    }

    let embed = CreateEmbed::new()
        .title("🎯 習慣一覧")
        .description(desc)
        .color(0x5865F2);

    let mut reply = poise::CreateReply::default().embed(embed);
    if !buttons.is_empty() {
        reply = reply.components(vec![serenity::CreateActionRow::Buttons(buttons)]);
    }

    ctx.send(reply).await?;
    Ok(())
}

/// 習慣の統計を表示
#[poise::command(slash_command)]
async fn stats(
    ctx: Context<'_>,
    #[description = "習慣名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let habit = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, created_at FROM habits WHERE user_id = ? AND guild_id = ? AND name = ? AND is_active = 1",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&name)
    .fetch_optional(&data.db)
    .await?;

    let Some((habit_id, created_at)) = habit else {
        ctx.say(format!("習慣「{}」が見つかりません。", name))
            .await?;
        return Ok(());
    };

    let streak = get_streak(&data.db, habit_id).await?;

    let week_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM habit_logs WHERE habit_id = ? AND checked_at >= date('now', '-7 days')",
    )
    .bind(habit_id)
    .fetch_one(&data.db)
    .await?;

    let month_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM habit_logs WHERE habit_id = ? AND checked_at >= date('now', '-30 days')",
    )
    .bind(habit_id)
    .fetch_one(&data.db)
    .await?;

    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM habit_logs WHERE habit_id = ?",
    )
    .bind(habit_id)
    .fetch_one(&data.db)
    .await?;

    let week_rate = (week_count as f64 / 7.0 * 100.0).min(100.0);
    let month_rate = (month_count as f64 / 30.0 * 100.0).min(100.0);

    let week_bar = progress_bar(week_rate);
    let month_bar = progress_bar(month_rate);

    let embed = CreateEmbed::new()
        .title(format!("📊 「{}」の統計", name))
        .color(0x5865F2)
        .field("🔥 連続", format!("**{}日**", streak), true)
        .field("📅 累計", format!("**{}回**", total), true)
        .field("開始日", &created_at, true)
        .field(
            format!("週間達成率 ({}/7)", week_count),
            format!("{} {:.0}%", week_bar, week_rate),
            false,
        )
        .field(
            format!("月間達成率 ({}/30)", month_count),
            format!("{} {:.0}%", month_bar, month_rate),
            false,
        );

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 習慣を削除
#[poise::command(slash_command)]
async fn remove(
    ctx: Context<'_>,
    #[description = "習慣名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let result = sqlx::query(
        "UPDATE habits SET is_active = 0 WHERE user_id = ? AND guild_id = ? AND name = ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&name)
    .execute(&data.db)
    .await?;

    if result.rows_affected() == 0 {
        ctx.say(format!("習慣「{}」が見つかりません。", name))
            .await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🗑️ 習慣削除")
                    .description(format!("「{}」を削除しました。", name))
                    .color(0xED4245),
            ),
        )
        .await?;
    }
    Ok(())
}

async fn get_streak(pool: &sqlx::SqlitePool, habit_id: i64) -> Result<i64, Error> {
    let logs = sqlx::query_scalar::<_, String>(
        "SELECT checked_at FROM habit_logs WHERE habit_id = ? ORDER BY checked_at DESC",
    )
    .bind(habit_id)
    .fetch_all(pool)
    .await?;

    let mut streak = 0i64;
    let today = chrono::Local::now().date_naive();

    for (i, log) in logs.iter().enumerate() {
        let date = chrono::NaiveDate::parse_from_str(log, "%Y-%m-%d").unwrap_or_default();
        let expected = today - chrono::Duration::days(i as i64);
        if date == expected {
            streak += 1;
        } else {
            break;
        }
    }

    Ok(streak)
}

fn progress_bar(percentage: f64) -> String {
    let filled = (percentage / 10.0).round() as usize;
    let empty = 10 - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}
