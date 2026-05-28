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
}

#[derive(Deserialize)]
struct Choice {
    message: ChatMessage,
}

/// AIに質問する
#[poise::command(slash_command, subcommands("new_conversation", "oneshot", "clear", "dispose"))]
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

    let provider = provider.unwrap_or_else(|| default_provider(&data.config));
    let (api_url, api_key, model_name) =
        resolve_api_config(provider, model, &data.config);

    call_ai_threaded(
        &data.http_client,
        &data.db,
        ctx.http(),
        thread_id,
        &ctx.author().id.to_string(),
        &question,
        &api_url,
        &api_key,
        &model_name,
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
    let provider = provider.unwrap_or_else(|| default_provider(&data.config));
    let (api_url, api_key, model_name) =
        resolve_api_config(provider, model, &data.config);

    let reply = call_ai_oneshot(
        &data.http_client,
        &question,
        &api_url,
        &api_key,
        &model_name,
    )
    .await?;

    send_embed_reply(ctx.channel_id(), ctx.http(), &reply, &model_name, "oneshot").await
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

// --- event_handler から呼ばれる公開関数 ---

pub async fn handle_thread_message(
    http: &serenity::Http,
    http_client: &reqwest::Client,
    db: &sqlx::SqlitePool,
    config: &crate::config::Config,
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

    let provider = default_provider(config);
    let (api_url, api_key, model_name) =
        resolve_api_config(provider, None, config);

    call_ai_threaded(
        http_client,
        db,
        http,
        channel_id,
        user_id,
        content,
        &api_url,
        &api_key,
        &model_name,
    )
    .await
}

pub async fn handle_mention(
    http: &serenity::Http,
    http_client: &reqwest::Client,
    config: &crate::config::Config,
    channel_id: serenity::ChannelId,
    content: &str,
) -> Result<(), Error> {
    let _ = channel_id.broadcast_typing(http).await;

    let provider = default_provider(config);
    let (api_url, api_key, model_name) =
        resolve_api_config(provider, None, config);

    let reply = call_ai_oneshot(http_client, content, &api_url, &api_key, &model_name).await?;

    send_embed_reply(channel_id, http, &reply, &model_name, "oneshot").await
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

    let reply = call_api(http_client, api_url, api_key, model_name, messages).await?;

    match reply {
        AiResult::Error(status, body) => {
            thread_id
                .send_message(
                    http,
                    serenity::CreateMessage::new().embed(
                        CreateEmbed::new()
                            .title("❌ APIエラー")
                            .description(format!("ステータス: {}\n```\n{}\n```", status, body))
                            .color(0xED4245),
                    ),
                )
                .await?;
        }
        AiResult::Ok(reply_text) => {
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

            let footer = format!("{} | {}ターン", model_name, history_count / 2);
            send_embed_reply(thread_id, http, &reply_text, model_name, &footer).await?;
        }
    }

    Ok(())
}

async fn call_ai_oneshot(
    http_client: &reqwest::Client,
    question: &str,
    api_url: &str,
    api_key: &str,
    model_name: &str,
) -> Result<String, Error> {
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: question.to_string(),
    }];

    match call_api(http_client, api_url, api_key, model_name, messages).await? {
        AiResult::Ok(text) => Ok(text),
        AiResult::Error(status, body) => {
            Err(format!("APIエラー ({}): {}", status, body).into())
        }
    }
}

enum AiResult {
    Ok(String),
    Error(reqwest::StatusCode, String),
}

async fn call_api(
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

    Ok(AiResult::Ok(reply))
}

async fn send_embed_reply(
    channel_id: serenity::ChannelId,
    http: &serenity::Http,
    text: &str,
    _model: &str,
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

fn default_provider(config: &crate::config::Config) -> AiProvider {
    match config.bot.default_ai_provider.as_str() {
        "openai" => AiProvider::OpenAI,
        "anthropic" => AiProvider::Anthropic,
        _ => AiProvider::OpenRouter,
    }
}

fn resolve_api_config(
    provider: AiProvider,
    model: Option<String>,
    config: &crate::config::Config,
) -> (String, String, String) {
    match provider {
        AiProvider::OpenRouter => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY not set"),
            model.unwrap_or_else(|| config.bot.default_model.clone()),
        ),
        AiProvider::OpenAI => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not set"),
            model.unwrap_or_else(|| "gpt-4o-mini".to_string()),
        ),
        AiProvider::Anthropic => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            std::env::var("OPENROUTER_API_KEY").expect("OPENROUTER_API_KEY not set"),
            model.unwrap_or_else(|| "anthropic/claude-sonnet-4-20250514".to_string()),
        ),
    }
}
