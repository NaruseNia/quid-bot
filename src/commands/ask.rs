use super::{Context, Error};
use poise::serenity_prelude::{self as serenity, CreateEmbed};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, poise::ChoiceParameter)]
pub enum AiProvider {
    #[name = "openrouter"]
    OpenRouter,
    #[name = "openai"]
    OpenAI,
    #[name = "anthropic"]
    Anthropic,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<Usage>,
}

#[derive(Deserialize, Clone, Copy, Default)]
struct Usage {
    prompt_tokens: i64,
    completion_tokens: i64,
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

/// AIに質問する
#[poise::command(
    slash_command,
    subcommands("new_conversation", "oneshot", "clear", "dispose", "usage", "pin", "save_thread", "load", "saved")
)]
pub async fn ask(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// 新しい会話を始める
#[poise::command(slash_command, rename = "new")]
async fn new_conversation(
    ctx: Context<'_>,
    #[description = "質問内容"] question: String,
    #[description = "AIプロバイダー"] provider: Option<AiProvider>,
    #[description = "モデル名"] model: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let data = ctx.data();
    let channel = ctx.channel_id();
    let is_thread = is_in_thread(ctx.serenity_context(), channel).await;

    let thread_id = if is_thread {
        channel
    } else {
        let thread_name: String = question.chars().take(40).collect();
        let thread_name = if question.len() > 40 {
            format!("{}...", thread_name)
        } else {
            thread_name
        };

        let reply = ctx
            .send(
                poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .description(format!("💬 **{}**", question))
                        .color(0x5865F2),
                ),
            )
            .await?;

        let msg = reply.message().await?;

        let thread = msg
            .channel_id
            .create_thread_from_message(
                ctx.http(),
                msg.id,
                serenity::CreateThread::new(thread_name)
                    .auto_archive_duration(serenity::AutoArchiveDuration::OneDay),
            )
            .await?;

        thread.id
    };

    let guild_id_str = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let provider_str = provider.map(provider_to_str);
    let ai = crate::ai::resolve(&data.db, &guild_id_str, &data.config, provider_str, model).await;

    call_ai_threaded(
        &data.http_client,
        &data.db,
        ctx.http(),
        thread_id,
        &ctx.author().id.to_string(),
        &question,
        &ai.api_url,
        &ai.api_key,
        &ai.model,
    )
    .await
}

/// 単発質問（スレッド作成なし・履歴保存なし）
#[poise::command(slash_command)]
async fn oneshot(
    ctx: Context<'_>,
    #[description = "質問内容"] question: String,
    #[description = "AIプロバイダー"] provider: Option<AiProvider>,
    #[description = "モデル名"] model: Option<String>,
    #[description = "参照するメッセージ数"] look: Option<u8>,
) -> Result<(), Error> {
    let channel_context = if let Some(count) = look {
        fetch_channel_context(ctx.http(), ctx.channel_id(), None, count.min(20)).await?
    } else {
        String::new()
    };

    let reply = ctx
        .send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .description("🤔 考え中...")
                    .color(0xE67E22),
            ),
        )
        .await?;

    let data = ctx.data();
    let guild_id_str = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let provider_str = provider.map(provider_to_str);
    let ai = crate::ai::resolve(&data.db, &guild_id_str, &data.config, provider_str, model).await;

    let author_name = ctx
        .author()
        .global_name
        .as_deref()
        .unwrap_or(&ctx.author().name);
    let full_question = if channel_context.is_empty() {
        question.clone()
    } else {
        format!(
            "あなたに話しかけているユーザーは {} です。\n\n{}{}",
            author_name, channel_context, question
        )
    };

    let result =
        call_api_raw(&data.http_client, &ai.api_url, &ai.api_key, &ai.model, vec![
            ChatMessage {
                role: "user".to_string(),
                content: full_question,
            },
        ])
        .await?;

    match result {
        AiResult::Ok(text, usage) => {
            record_usage(&data.db, &ctx.author().id.to_string(), &ai.model, usage).await;
            let footer = format_footer(&ai.model, "oneshot", usage);
            edit_reply_with_response(&reply, ctx, &text, &footer).await
        }
        AiResult::Error(status, body) => {
            reply
                .edit(
                    ctx,
                    poise::CreateReply::default().embed(
                        CreateEmbed::new()
                            .description(format!("❌ APIエラー ({}): {}", status, body))
                            .color(0xED4245),
                    ),
                )
                .await?;
            Ok(())
        }
    }
}

/// このスレッドの会話履歴をクリア
#[poise::command(slash_command)]
async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let thread_id = ctx.channel_id().to_string();

    let result = sqlx::query("DELETE FROM conversations WHERE thread_id = ?")
        .bind(&thread_id)
        .execute(&data.db)
        .await?;

    let deleted = result.rows_affected();
    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🧹 会話履歴クリア")
                .description(format!(
                    "{}件のメッセージを削除しました。\nこのスレッドで新しい会話を始められます。",
                    deleted
                ))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// スレッドをアーカイブ（保存済みなら履歴保持、未保存なら削除）
#[poise::command(slash_command)]
async fn dispose(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let channel_id = ctx.channel_id();

    if !is_in_thread(ctx.serenity_context(), channel_id).await {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("このコマンドはスレッド内でのみ使用できます。")
                .color(0xED4245),
        )).await?;
        return Ok(());
    }

    let thread_id_str = channel_id.to_string();

    let is_saved = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM saved_threads WHERE thread_id = ?",
    )
    .bind(&thread_id_str)
    .fetch_one(&data.db)
    .await? > 0;

    if is_saved {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("📦 スレッドをアーカイブ")
                    .description("保存済みの会話です。要約と履歴を保持したままアーカイブします。\n`/ask load` で再開できます。")
                    .color(0x57F287),
            ),
        )
        .await?;
    } else {
        sqlx::query("DELETE FROM conversations WHERE thread_id = ?")
            .bind(&thread_id_str)
            .execute(&data.db)
            .await?;

        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("📦 スレッド終了")
                    .description("会話履歴を削除し、アーカイブします。\n保存したい場合は先に `/ask save <名前>` を使ってください。")
                    .color(0x99AAB5),
            ),
        )
        .await?;
    }

    let edit = serenity::EditThread::new().archived(true).locked(true);
    channel_id.edit_thread(ctx.http(), edit).await?;

    Ok(())
}

/// スレッドをピン留め（自動アーカイブを最大に延長）
#[poise::command(slash_command)]
async fn pin(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let channel_id = ctx.channel_id();

    if !is_in_thread(ctx.serenity_context(), channel_id).await {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().description("このコマンドはスレッド内でのみ使用できます。").color(0xED4245),
        )).await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let edit = serenity::EditThread::new()
        .auto_archive_duration(serenity::AutoArchiveDuration::OneWeek);
    channel_id.edit_thread(ctx.http(), edit).await?;

    sqlx::query(
        "INSERT INTO saved_threads (user_id, guild_id, thread_id, name, pinned) VALUES (?, ?, ?, ?, 1) ON CONFLICT(thread_id) DO UPDATE SET pinned = 1",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(channel_id.to_string())
    .bind("pinned")
    .execute(&data.db)
    .await?;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("📌 スレッドをピン留め")
            .description("自動アーカイブを1週間に延長しました。")
            .color(0x57F287),
    )).await?;
    Ok(())
}

/// 会話をAI要約して名前付き保存（フル履歴は削除してDB軽量化）
#[poise::command(slash_command, rename = "save")]
async fn save_thread(
    ctx: Context<'_>,
    #[description = "保存名"] name: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let data = ctx.data();
    let channel_id = ctx.channel_id();

    if !is_in_thread(ctx.serenity_context(), channel_id).await {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().description("このコマンドはスレッド内でのみ使用できます。").color(0xED4245),
        )).await?;
        return Ok(());
    }

    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let thread_id_str = channel_id.to_string();

    let history = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM conversations WHERE thread_id = ? ORDER BY created_at ASC",
    )
    .bind(&thread_id_str)
    .fetch_all(&data.db)
    .await?;

    let msg_count = history.len();

    let summary = if history.is_empty() {
        "(会話履歴なし)".to_string()
    } else {
        let conversation: String = history
            .iter()
            .map(|(role, content)| format!("{}: {}", role, content))
            .collect::<Vec<_>>()
            .join("\n");

        generate_summary(&data.http_client, &data.db, &guild_id, &data.config, &conversation).await
    };

    sqlx::query(
        "INSERT INTO saved_threads (user_id, guild_id, thread_id, name, summary) VALUES (?, ?, ?, ?, ?) ON CONFLICT(thread_id) DO UPDATE SET name = excluded.name, summary = excluded.summary",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&thread_id_str)
    .bind(&name)
    .bind(&summary)
    .execute(&data.db)
    .await?;

    // フル履歴を削除してDB軽量化
    sqlx::query("DELETE FROM conversations WHERE thread_id = ?")
        .bind(&thread_id_str)
        .execute(&data.db)
        .await?;

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("💾 会話を保存")
            .description(format!("**{}** として保存しました。", name))
            .field("元メッセージ数", format!("{}件", msg_count), true)
            .field("要約", &summary, false)
            .color(0x57F287),
    )).await?;
    Ok(())
}

/// 保存済み会話を復元（要約をコンテキストとして注入）
#[poise::command(slash_command)]
async fn load(
    ctx: Context<'_>,
    #[description = "保存名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let thread = sqlx::query_as::<_, (String, bool, Option<String>)>(
        "SELECT thread_id, pinned, summary FROM saved_threads WHERE user_id = ? AND guild_id = ? AND name = ?",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .bind(&name)
    .fetch_optional(&data.db)
    .await?;

    let Some((thread_id, pinned, summary)) = thread else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new().description(format!("保存済み会話「{}」が見つかりません。", name)).color(0xED4245),
        )).await?;
        return Ok(());
    };

    let thread_channel: serenity::ChannelId = thread_id.parse::<u64>().unwrap_or(0).into();

    // アーカイブ・ロック解除
    let edit = serenity::EditThread::new().archived(false).locked(false);
    thread_channel.edit_thread(ctx.http(), edit).await.ok();

    // 要約をシステムメッセージとしてDB注入（会話の文脈を復元）
    if let Some(ref s) = summary {
        let existing = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM conversations WHERE thread_id = ?",
        )
        .bind(&thread_id)
        .fetch_one(&data.db)
        .await?;

        if existing == 0 {
            let context_msg = format!("以下は前回の会話の要約です。この文脈を踏まえて会話を続けてください:\n\n{}", s);
            sqlx::query(
                "INSERT INTO conversations (user_id, thread_id, role, content, model) VALUES (?, ?, 'system', ?, 'system')",
            )
            .bind(ctx.author().id.to_string())
            .bind(&thread_id)
            .bind(&context_msg)
            .execute(&data.db)
            .await?;
        }
    }

    let pin_icon = if pinned { " 📌" } else { "" };
    let mut embed = CreateEmbed::new()
        .title(format!("💬 {}{}", name, pin_icon))
        .description(format!("スレッドを再開しました: <#{}>", thread_id))
        .color(0x5865F2);

    if let Some(ref s) = summary {
        let preview = if s.len() > 300 {
            format!("{}...", &s[..300])
        } else {
            s.clone()
        };
        embed = embed.field("前回の要約", preview, false);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 保存済み会話スレッド一覧
#[poise::command(slash_command)]
async fn saved(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let threads = sqlx::query_as::<_, (String, String, bool, String)>(
        "SELECT thread_id, name, pinned, created_at FROM saved_threads WHERE user_id = ? AND guild_id = ? ORDER BY created_at DESC",
    )
    .bind(ctx.author().id.to_string())
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if threads.is_empty() {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("💬 保存済み会話")
                .description("保存済みの会話はありません。\nスレッド内で `/ask save <名前>` で保存できます。")
                .color(0x99AAB5),
        )).await?;
        return Ok(());
    }

    let desc: String = threads
        .iter()
        .map(|(thread_id, name, pinned, date)| {
            let pin = if *pinned { " 📌" } else { "" };
            let date_short = &date[..10.min(date.len())];
            format!("**{}**{} — <#{}> ({})", name, pin, thread_id, date_short)
        })
        .collect::<Vec<_>>()
        .join("\n");

    ctx.send(poise::CreateReply::default().embed(
        CreateEmbed::new()
            .title("💬 保存済み会話")
            .description(desc)
            .color(0x5865F2),
    )).await?;
    Ok(())
}

/// AI利用量の統計を表示
#[poise::command(slash_command)]
async fn usage(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let user_id = ctx.author().id.to_string();

    let total = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COUNT(*), COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0) FROM ai_usage WHERE user_id = ?",
    )
    .bind(&user_id)
    .fetch_one(&data.db)
    .await?;

    let today = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT COUNT(*), COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0) FROM ai_usage WHERE user_id = ? AND date(created_at) = date('now')",
    )
    .bind(&user_id)
    .fetch_one(&data.db)
    .await?;

    let by_model = sqlx::query_as::<_, (String, i64, i64, i64)>(
        "SELECT model, COUNT(*), COALESCE(SUM(prompt_tokens), 0), COALESCE(SUM(completion_tokens), 0) FROM ai_usage WHERE user_id = ? GROUP BY model ORDER BY COUNT(*) DESC LIMIT 5",
    )
    .bind(&user_id)
    .fetch_all(&data.db)
    .await?;

    let mut embed = CreateEmbed::new()
        .title("📊 AI利用統計")
        .color(0x5865F2)
        .field(
            "今日",
            format!(
                "{}回 | {}+{} = {} tokens",
                today.0,
                today.1,
                today.2,
                today.1 + today.2
            ),
            false,
        )
        .field(
            "累計",
            format!(
                "{}回 | {}+{} = {} tokens",
                total.0,
                total.1,
                total.2,
                total.1 + total.2
            ),
            false,
        );

    if !by_model.is_empty() {
        let model_stats: String = by_model
            .iter()
            .map(|(model, count, prompt, completion)| {
                format!(
                    "`{}` — {}回 ({} tokens)",
                    model,
                    count,
                    prompt + completion
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.field("モデル別", model_stats, false);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

// --- event_handler から呼ばれる公開関数 ---

#[allow(clippy::too_many_arguments)]
pub async fn handle_thread_message(
    http: &serenity::Http,
    http_client: &reqwest::Client,
    db: &sqlx::SqlitePool,
    config: &crate::config::Config,
    guild_id: &str,
    channel_id: serenity::ChannelId,
    user_id: &str,
    content: &str,
) -> Result<(), Error> {
    let thread_id_str = channel_id.to_string();

    let count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversations WHERE thread_id = ?",
    )
    .bind(&thread_id_str)
    .fetch_one(db)
    .await?;

    if count == 0 {
        return Ok(());
    }

    let ai = crate::ai::resolve(db, guild_id, config, None, None).await;

    call_ai_threaded(
        http_client,
        db,
        http,
        channel_id,
        user_id,
        content,
        &ai.api_url,
        &ai.api_key,
        &ai.model,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub async fn handle_mention(
    http: &serenity::Http,
    http_client: &reqwest::Client,
    db: &sqlx::SqlitePool,
    config: &crate::config::Config,
    guild_id: &str,
    channel_id: serenity::ChannelId,
    user_id: &str,
    content: &str,
    before_message: serenity::MessageId,
    referenced_content: Option<&str>,
    author_name: &str,
) -> Result<(), Error> {
    let (look_count, clean_content) = parse_look_prefix(content);

    let thinking_msg = channel_id
        .send_message(
            http,
            serenity::CreateMessage::new().embed(
                CreateEmbed::new()
                    .description("🤔 考え中...")
                    .color(0xE67E22),
            ),
        )
        .await?;

    let mut prompt = String::new();

    let has_context = referenced_content.is_some() || look_count.is_some();
    if has_context {
        prompt.push_str(&format!(
            "あなたに話しかけているユーザーは {} です。\n\n",
            author_name
        ));
    }

    if let Some(ref_content) = referenced_content {
        prompt.push_str(&format!(
            "以下はユーザーが返信元として参照しているメッセージです:\n---\n{}\n---\n\n",
            ref_content
        ));
    }

    if let Some(count) = look_count {
        let ctx = fetch_channel_context(http, channel_id, Some(before_message), count).await?;
        prompt.push_str(&ctx);
    }

    prompt.push_str(&clean_content);

    let ai = crate::ai::resolve(db, guild_id, config, None, None).await;

    let result = call_api_raw(http_client, &ai.api_url, &ai.api_key, &ai.model, vec![
        ChatMessage {
            role: "user".to_string(),
            content: prompt,
        },
    ])
    .await?;

    match result {
        AiResult::Ok(text, usage) => {
            record_usage(db, user_id, &ai.model, usage).await;
            let footer = format_footer(&ai.model, "oneshot", usage);
            edit_to_response(channel_id, http, thinking_msg.id, &text, &footer).await
        }
        AiResult::Error(status, body) => {
            channel_id
                .edit_message(
                    http,
                    thinking_msg.id,
                    serenity::EditMessage::new().embed(
                        CreateEmbed::new()
                            .title("❌ APIエラー")
                            .description(format!("{}: {}", status, body))
                            .color(0xED4245),
                    ),
                )
                .await?;
            Ok(())
        }
    }
}

// --- 内部ヘルパー ---

#[allow(clippy::too_many_arguments)]
async fn call_ai_threaded(
    http_client: &reqwest::Client,
    db: &sqlx::SqlitePool,
    http: &serenity::Http,
    thread_id: serenity::ChannelId,
    user_id: &str,
    question: &str,
    api_url: &str,
    api_key: &str,
    model_name: &str,
) -> Result<(), Error> {
    let thinking_msg = thread_id
        .send_message(
            http,
            serenity::CreateMessage::new().embed(
                CreateEmbed::new()
                    .description("🤔 考え中...")
                    .color(0xE67E22),
            ),
        )
        .await?;

    let thread_id_str = thread_id.to_string();

    let history = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM conversations WHERE thread_id = ? ORDER BY created_at ASC LIMIT 20",
    )
    .bind(&thread_id_str)
    .fetch_all(db)
    .await?;

    let mut messages: Vec<ChatMessage> = history
        .into_iter()
        .map(|(role, content)| ChatMessage { role, content })
        .collect();
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: question.to_string(),
    });

    let result = call_api_raw(http_client, api_url, api_key, model_name, messages).await?;

    match result {
        AiResult::Error(status, body) => {
            thread_id
                .edit_message(
                    http,
                    thinking_msg.id,
                    serenity::EditMessage::new().embed(
                        CreateEmbed::new()
                            .title("❌ APIエラー")
                            .description(format!("{}: {}", status, body))
                            .color(0xED4245),
                    ),
                )
                .await?;
        }
        AiResult::Ok(reply_text, usage) => {
            record_usage(db, user_id, model_name, usage).await;

            sqlx::query(
                "INSERT INTO conversations (user_id, thread_id, role, content, model) VALUES (?, ?, 'user', ?, ?)",
            )
            .bind(user_id)
            .bind(&thread_id_str)
            .bind(question)
            .bind(model_name)
            .execute(db)
            .await?;

            sqlx::query(
                "INSERT INTO conversations (user_id, thread_id, role, content, model) VALUES (?, ?, 'assistant', ?, ?)",
            )
            .bind(user_id)
            .bind(&thread_id_str)
            .bind(&reply_text)
            .bind(model_name)
            .execute(db)
            .await?;

            let history_count = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM conversations WHERE thread_id = ?",
            )
            .bind(&thread_id_str)
            .fetch_one(db)
            .await?;

            let footer = format_footer(
                model_name,
                &format!("{}ターン", history_count / 2),
                usage,
            );
            edit_to_response(thread_id, http, thinking_msg.id, &reply_text, &footer).await?;
        }
    }

    Ok(())
}

enum AiResult {
    Ok(String, Usage),
    Error(reqwest::StatusCode, String),
}

async fn call_api_raw(
    http_client: &reqwest::Client,
    api_url: &str,
    api_key: &str,
    model_name: &str,
    messages: Vec<ChatMessage>,
) -> Result<AiResult, Error> {
    let request = ChatRequest {
        model: model_name.to_string(),
        messages,
    };

    let response = http_client
        .post(api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Ok(AiResult::Error(status, body));
    }

    let chat_response: ChatResponse = response.json().await?;
    let reply = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "応答がありませんでした。".to_string());

    let usage = chat_response.usage.unwrap_or_default();

    Ok(AiResult::Ok(reply, usage))
}

async fn record_usage(db: &sqlx::SqlitePool, user_id: &str, model: &str, usage: Usage) {
    sqlx::query(
        "INSERT INTO ai_usage (user_id, model, prompt_tokens, completion_tokens) VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(model)
    .bind(usage.prompt_tokens)
    .bind(usage.completion_tokens)
    .execute(db)
    .await
    .ok();
}

fn format_footer(model: &str, context: &str, usage: Usage) -> String {
    let total = usage.prompt_tokens + usage.completion_tokens;
    if total > 0 {
        format!(
            "{} | {} | {}+{}={} tokens",
            model, context, usage.prompt_tokens, usage.completion_tokens, total
        )
    } else {
        format!("{} | {}", model, context)
    }
}

async fn edit_to_response(
    channel_id: serenity::ChannelId,
    http: &serenity::Http,
    message_id: serenity::MessageId,
    text: &str,
    footer: &str,
) -> Result<(), Error> {
    if text.len() > 4000 {
        channel_id
            .edit_message(
                http,
                message_id,
                serenity::EditMessage::new().embed(
                    CreateEmbed::new()
                        .description(&text[..4000])
                        .color(0x5865F2)
                        .footer(serenity::CreateEmbedFooter::new(footer)),
                ),
            )
            .await?;
        for chunk in text[4000..].as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            channel_id
                .send_message(
                    http,
                    serenity::CreateMessage::new().embed(
                        CreateEmbed::new()
                            .description(chunk_str.to_string())
                            .color(0x5865F2),
                    ),
                )
                .await?;
        }
    } else {
        channel_id
            .edit_message(
                http,
                message_id,
                serenity::EditMessage::new().embed(
                    CreateEmbed::new()
                        .description(text)
                        .color(0x5865F2)
                        .footer(serenity::CreateEmbedFooter::new(footer)),
                ),
            )
            .await?;
    }
    Ok(())
}

async fn edit_reply_with_response(
    reply: &poise::ReplyHandle<'_>,
    ctx: Context<'_>,
    text: &str,
    footer: &str,
) -> Result<(), Error> {
    if text.len() > 4000 {
        reply
            .edit(
                ctx,
                poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .description(&text[..4000])
                        .color(0x5865F2)
                        .footer(serenity::CreateEmbedFooter::new(footer)),
                ),
            )
            .await?;
        for chunk in text[4000..].as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            ctx.channel_id()
                .send_message(
                    ctx.http(),
                    serenity::CreateMessage::new().embed(
                        CreateEmbed::new()
                            .description(chunk_str.to_string())
                            .color(0x5865F2),
                    ),
                )
                .await?;
        }
    } else {
        reply
            .edit(
                ctx,
                poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .description(text)
                        .color(0x5865F2)
                        .footer(serenity::CreateEmbedFooter::new(footer)),
                ),
            )
            .await?;
    }
    Ok(())
}

fn display_name_from_message(m: &serenity::Message) -> String {
    let base = m
        .member
        .as_ref()
        .and_then(|member| member.nick.as_deref())
        .or(m.author.global_name.as_deref())
        .unwrap_or(&m.author.name);
    if m.author.bot {
        format!("[BOT] {}", base)
    } else {
        base.to_string()
    }
}

fn parse_look_prefix(content: &str) -> (Option<u8>, String) {
    let trimmed = content.trim();
    if let Some(rest) = trimmed.strip_prefix("!look:") {
        let num_end = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if num_end > 0 {
            if let Ok(n) = rest[..num_end].parse::<u8>() {
                let remaining = rest[num_end..].trim().to_string();
                return (Some(n.min(20)), remaining);
            }
        }
    }
    (None, content.to_string())
}

async fn fetch_channel_context(
    http: &serenity::Http,
    channel_id: serenity::ChannelId,
    before: Option<serenity::MessageId>,
    count: u8,
) -> Result<String, Error> {
    let builder = if let Some(id) = before {
        serenity::GetMessages::new().before(id).limit(count)
    } else {
        serenity::GetMessages::new().limit(count)
    };

    let messages = channel_id.messages(http, builder).await?;

    if messages.is_empty() {
        return Ok(String::new());
    }

    let lines: Vec<String> = messages
        .iter()
        .rev()
        .map(|m| {
            let name = display_name_from_message(m);
            format!("{}: {}", name, m.content)
        })
        .collect();

    Ok(format!(
        "以下はチャンネルの直近の会話です（参考情報）:\n---\n{}\n---\n\n",
        lines.join("\n")
    ))
}

async fn is_in_thread(ctx: &serenity::Context, channel_id: serenity::ChannelId) -> bool {
    if let Ok(channel) = channel_id.to_channel(ctx).await {
        matches!(
            channel,
            serenity::Channel::Guild(gc) if gc.kind == serenity::ChannelType::PublicThread
                || gc.kind == serenity::ChannelType::PrivateThread
        )
    } else {
        false
    }
}

pub fn is_thread_channel(channel: &serenity::Channel) -> bool {
    matches!(
        channel,
        serenity::Channel::Guild(gc) if gc.kind == serenity::ChannelType::PublicThread
            || gc.kind == serenity::ChannelType::PrivateThread
    )
}

fn provider_to_str(p: AiProvider) -> &'static str {
    match p {
        AiProvider::OpenRouter => "openrouter",
        AiProvider::OpenAI => "openai",
        AiProvider::Anthropic => "anthropic",
    }
}

async fn generate_summary(
    http_client: &reqwest::Client,
    db: &sqlx::SqlitePool,
    guild_id: &str,
    config: &crate::config::Config,
    conversation: &str,
) -> String {
    let ai = crate::ai::resolve(db, guild_id, config, None, None).await;
    if ai.api_key.is_empty() {
        return "(要約生成不可: APIキー未設定)".to_string();
    }

    let truncated = if conversation.len() > 6000 {
        &conversation[..6000]
    } else {
        conversation
    };

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: format!(
            "以下の会話を3-5文で要約してください。重要なトピック、決定事項、コンテキストを保持してください。\n\n{}",
            truncated
        ),
    }];

    match call_api_raw(http_client, &ai.api_url, &ai.api_key, &ai.model, messages).await {
        Ok(AiResult::Ok(text, _)) => text,
        _ => "(要約の生成に失敗しました)".to_string(),
    }
}
