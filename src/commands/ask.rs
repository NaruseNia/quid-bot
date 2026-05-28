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
    subcommands("new_conversation", "oneshot", "clear", "dispose", "usage")
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
) -> Result<(), Error> {
    ctx.defer().await?;

    let data = ctx.data();
    let guild_id_str = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let provider_str = provider.map(provider_to_str);
    let ai = crate::ai::resolve(&data.db, &guild_id_str, &data.config, provider_str, model).await;

    let result =
        call_api_raw(&data.http_client, &ai.api_url, &ai.api_key, &ai.model, vec![
            ChatMessage {
                role: "user".to_string(),
                content: question.clone(),
            },
        ])
        .await?;

    match result {
        AiResult::Ok(text, usage) => {
            record_usage(&data.db, &ctx.author().id.to_string(), &ai.model, usage).await;
            let footer = format_footer(&ai.model, "oneshot", usage);
            send_embed_reply(ctx.channel_id(), ctx.http(), &text, &footer).await
        }
        AiResult::Error(status, body) => {
            ctx.say(format!("❌ APIエラー ({}): {}", status, body))
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

/// スレッドをアーカイブして会話履歴を削除
#[poise::command(slash_command)]
async fn dispose(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let channel_id = ctx.channel_id();

    if !is_in_thread(ctx.serenity_context(), channel_id).await {
        ctx.say("このコマンドはスレッド内でのみ使用できます。")
            .await?;
        return Ok(());
    }

    let thread_id_str = channel_id.to_string();
    sqlx::query("DELETE FROM conversations WHERE thread_id = ?")
        .bind(&thread_id_str)
        .execute(&data.db)
        .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📦 スレッド終了")
                .description("会話履歴を削除し、スレッドをアーカイブします。")
                .color(0x99AAB5),
        ),
    )
    .await?;

    let edit = serenity::EditThread::new().archived(true).locked(true);
    channel_id.edit_thread(ctx.http(), edit).await?;

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

    let _ = channel_id.broadcast_typing(http).await;

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
) -> Result<(), Error> {
    let _ = channel_id.broadcast_typing(http).await;

    let ai = crate::ai::resolve(db, guild_id, config, None, None).await;

    let result = call_api_raw(http_client, &ai.api_url, &ai.api_key, &ai.model, vec![
        ChatMessage {
            role: "user".to_string(),
            content: content.to_string(),
        },
    ])
    .await?;

    match result {
        AiResult::Ok(text, usage) => {
            record_usage(db, user_id, &ai.model, usage).await;
            let footer = format_footer(&ai.model, "oneshot", usage);
            send_embed_reply(channel_id, http, &text, &footer).await
        }
        AiResult::Error(status, body) => {
            channel_id
                .send_message(
                    http,
                    serenity::CreateMessage::new().embed(
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
                .send_message(
                    http,
                    serenity::CreateMessage::new().embed(
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
            send_embed_reply(thread_id, http, &reply_text, &footer).await?;
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

async fn send_embed_reply(
    channel_id: serenity::ChannelId,
    http: &serenity::Http,
    text: &str,
    footer: &str,
) -> Result<(), Error> {
    if text.len() > 4000 {
        for chunk in text.as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            channel_id
                .send_message(
                    http,
                    serenity::CreateMessage::new().embed(
                        CreateEmbed::new()
                            .description(chunk_str.to_string())
                            .color(0x5865F2)
                            .footer(serenity::CreateEmbedFooter::new(footer)),
                    ),
                )
                .await?;
        }
    } else {
        channel_id
            .send_message(
                http,
                serenity::CreateMessage::new().embed(
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
