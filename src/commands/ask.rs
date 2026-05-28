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
#[poise::command(slash_command, subcommands("chat", "oneshot", "clear"))]
pub async fn ask(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// AIに質問する（スレッド内で会話継続）
#[poise::command(slash_command)]
async fn chat(
    ctx: Context<'_>,
    #[description = "質問内容"] question: String,
    #[description = "AIプロバイダー"] provider: Option<AiProvider>,
    #[description = "モデル名"] model: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let data = ctx.data();
    let provider = provider.unwrap_or(match data.config.bot.default_ai_provider.as_str() {
        "openai" => AiProvider::OpenAI,
        "anthropic" => AiProvider::Anthropic,
        _ => AiProvider::OpenRouter,
    });

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

    let thread_id_str = thread_id.to_string();

    let history = sqlx::query_as::<_, (String, String)>(
        "SELECT role, content FROM conversations WHERE thread_id = ? ORDER BY created_at ASC LIMIT 20",
    )
    .bind(&thread_id_str)
    .fetch_all(&data.db)
    .await?;

    let mut messages: Vec<ChatMessage> = history
        .into_iter()
        .map(|(role, content)| ChatMessage { role, content })
        .collect();
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: question.clone(),
    });

    let (api_url, api_key, model_name) = resolve_api_config(provider, model, &data.config);

    let request = ChatRequest {
        model: model_name.clone(),
        messages: messages.clone(),
    };

    let response = data
        .http_client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        thread_id
            .send_message(
                ctx.http(),
                serenity::CreateMessage::new().embed(
                    CreateEmbed::new()
                        .title("❌ APIエラー")
                        .description(format!("ステータス: {}\n```\n{}\n```", status, body))
                        .color(0xED4245),
                ),
            )
            .await?;
        return Ok(());
    }

    let chat_response: ChatResponse = response.json().await?;
    let reply = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "応答がありませんでした。".to_string());

    sqlx::query(
        "INSERT INTO conversations (user_id, thread_id, role, content, model) VALUES (?, ?, 'user', ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&thread_id_str)
    .bind(&question)
    .bind(&model_name)
    .execute(&data.db)
    .await?;

    sqlx::query(
        "INSERT INTO conversations (user_id, thread_id, role, content, model) VALUES (?, ?, 'assistant', ?, ?)",
    )
    .bind(ctx.author().id.to_string())
    .bind(&thread_id_str)
    .bind(&reply)
    .bind(&model_name)
    .execute(&data.db)
    .await?;

    let history_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM conversations WHERE thread_id = ?",
    )
    .bind(&thread_id_str)
    .fetch_one(&data.db)
    .await?;

    let footer_text = format!("{} | {}ターン", model_name, history_count / 2);

    if reply.len() > 4000 {
        for chunk in reply.as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            thread_id
                .send_message(
                    ctx.http(),
                    serenity::CreateMessage::new().embed(
                        CreateEmbed::new()
                            .description(chunk_str.to_string())
                            .color(0x5865F2)
                            .footer(serenity::CreateEmbedFooter::new(&footer_text)),
                    ),
                )
                .await?;
        }
    } else {
        thread_id
            .send_message(
                ctx.http(),
                serenity::CreateMessage::new().embed(
                    CreateEmbed::new()
                        .description(&reply)
                        .color(0x5865F2)
                        .footer(serenity::CreateEmbedFooter::new(&footer_text)),
                ),
            )
            .await?;
    }

    Ok(())
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
    let provider = provider.unwrap_or(match data.config.bot.default_ai_provider.as_str() {
        "openai" => AiProvider::OpenAI,
        "anthropic" => AiProvider::Anthropic,
        _ => AiProvider::OpenRouter,
    });

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: question.clone(),
    }];

    let (api_url, api_key, model_name) = resolve_api_config(provider, model, &data.config);

    let request = ChatRequest {
        model: model_name.clone(),
        messages,
    };

    let response = data
        .http_client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("❌ APIエラー")
                    .description(format!("ステータス: {}\n```\n{}\n```", status, body))
                    .color(0xED4245),
            ),
        )
        .await?;
        return Ok(());
    }

    let chat_response: ChatResponse = response.json().await?;
    let reply = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "応答がありませんでした。".to_string());

    if reply.len() > 4000 {
        for chunk in reply.as_bytes().chunks(4000) {
            let chunk_str = String::from_utf8_lossy(chunk);
            ctx.send(
                poise::CreateReply::default().embed(
                    CreateEmbed::new()
                        .description(chunk_str.to_string())
                        .color(0x5865F2)
                        .footer(serenity::CreateEmbedFooter::new(format!(
                            "{} | oneshot",
                            model_name
                        ))),
                ),
            )
            .await?;
        }
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .description(&reply)
                    .color(0x5865F2)
                    .footer(serenity::CreateEmbedFooter::new(format!(
                        "{} | oneshot",
                        model_name
                    ))),
            ),
        )
        .await?;
    }

    Ok(())
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
