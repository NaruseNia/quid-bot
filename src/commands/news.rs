use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;

struct PresetFeed {
    category: &'static str,
    name: &'static str,
    url: &'static str,
}

const PRESET_FEEDS: &[PresetFeed] = &[
    PresetFeed { category: "tech", name: "Hacker News", url: "https://hnrss.org/frontpage?count=10" },
    PresetFeed { category: "tech", name: "Zenn Trend", url: "https://zenn.dev/feed" },
    PresetFeed { category: "world", name: "NHK World", url: "https://www3.nhk.or.jp/rss/news/cat6.xml" },
    PresetFeed { category: "world", name: "BBC News", url: "http://feeds.bbci.co.uk/news/world/rss.xml" },
    PresetFeed { category: "japan", name: "NHK 主要", url: "https://www3.nhk.or.jp/rss/news/cat0.xml" },
    PresetFeed { category: "japan", name: "Yahoo! 主要", url: "https://news.yahoo.co.jp/rss/topics/top-picks.xml" },
    PresetFeed { category: "business", name: "日経", url: "https://assets.wor.jp/rss/rdf/nikkei/news.rdf" },
    PresetFeed { category: "business", name: "Bloomberg JP", url: "https://www.bloomberg.co.jp/feeds/sitemap_news.xml" },
];

pub const CATEGORIES: &[&str] = &["tech", "world", "japan", "business"];

#[derive(Debug, Clone)]
pub struct FeedEntry {
    pub title: String,
    pub link: String,
}

/// ニュース
#[poise::command(slash_command, subcommands("show", "add", "remove", "list_feeds"))]
pub async fn news(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// ニュースを表示
#[poise::command(slash_command, rename = "show")]
async fn show(
    ctx: Context<'_>,
    #[description = "カテゴリ (tech/world/japan/business) またはカスタム名"] category: String,
    #[description = "AI要約を付ける"] summary: Option<bool>,
    #[description = "表示件数 (デフォルト5)"] count: Option<usize>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();
    let count = count.unwrap_or(5);
    let summary = summary.unwrap_or(false);

    let urls = get_feed_urls(&data.db, &guild_id, &category).await?;
    if urls.is_empty() {
        ctx.say(format!("カテゴリ「{}」にフィードがありません。", category))
            .await?;
        return Ok(());
    }

    let entries = fetch_feeds(&data.http_client, &urls, count).await;

    if entries.is_empty() {
        ctx.say("ニュースを取得できませんでした。").await?;
        return Ok(());
    }

    let mut embed = CreateEmbed::new()
        .title(format!("📰 {} ニュース", category))
        .color(0x5865F2);

    if summary {
        let titles: String = entries
            .iter()
            .map(|e| format!("- {}", e.title))
            .collect::<Vec<_>>()
            .join("\n");

        let summary_text =
            generate_news_summary(&data.http_client, &data.config, &titles).await;
        embed = embed.description(summary_text);
        embed = embed.field(
            "元記事",
            entries
                .iter()
                .map(|e| format!("[{}]({})", truncate(&e.title, 50), e.link))
                .collect::<Vec<_>>()
                .join("\n"),
            false,
        );
    } else {
        let desc: String = entries
            .iter()
            .enumerate()
            .map(|(i, e)| format!("**{}.**  [{}]({})", i + 1, e.title, e.link))
            .collect::<Vec<_>>()
            .join("\n");
        embed = embed.description(desc);
    }

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// カスタムRSSフィードを登録
#[poise::command(slash_command)]
async fn add(
    ctx: Context<'_>,
    #[description = "フィード名"] name: String,
    #[description = "RSS URL"] url: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    sqlx::query(
        "INSERT INTO custom_feeds (guild_id, name, url) VALUES (?, ?, ?) ON CONFLICT(guild_id, name) DO UPDATE SET url = excluded.url",
    )
    .bind(&guild_id)
    .bind(&name)
    .bind(&url)
    .execute(&data.db)
    .await?;

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("✅ フィード登録")
                .description(format!("「{}」を登録しました。\n`/news show {}` で表示できます。", name, name))
                .color(0x57F287),
        ),
    )
    .await?;
    Ok(())
}

/// カスタムRSSフィードを削除
#[poise::command(slash_command)]
async fn remove(
    ctx: Context<'_>,
    #[description = "フィード名"] name: String,
) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let result = sqlx::query("DELETE FROM custom_feeds WHERE guild_id = ? AND name = ?")
        .bind(&guild_id)
        .bind(&name)
        .execute(&data.db)
        .await?;

    if result.rows_affected() == 0 {
        ctx.say(format!("フィード「{}」が見つかりません。", name))
            .await?;
    } else {
        ctx.send(
            poise::CreateReply::default().embed(
                CreateEmbed::new()
                    .title("🗑️ フィード削除")
                    .description(format!("「{}」を削除しました。", name))
                    .color(0xED4245),
            ),
        )
        .await?;
    }
    Ok(())
}

/// 登録フィード一覧
#[poise::command(slash_command, rename = "list")]
async fn list_feeds(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data();
    let guild_id = ctx.guild_id().map(|g| g.to_string()).unwrap_or_default();

    let mut desc = String::from("**プリセット**\n");
    for cat in CATEGORIES {
        let feeds: Vec<&str> = PRESET_FEEDS
            .iter()
            .filter(|f| f.category == *cat)
            .map(|f| f.name)
            .collect();
        desc.push_str(&format!("`{}` — {}\n", cat, feeds.join(", ")));
    }

    let custom = sqlx::query_as::<_, (String, String)>(
        "SELECT name, url FROM custom_feeds WHERE guild_id = ? ORDER BY name",
    )
    .bind(&guild_id)
    .fetch_all(&data.db)
    .await?;

    if !custom.is_empty() {
        desc.push_str("\n**カスタム**\n");
        for (name, url) in &custom {
            desc.push_str(&format!("`{}` — {}\n", name, url));
        }
    }

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("📰 フィード一覧")
                .description(desc)
                .color(0x5865F2),
        ),
    )
    .await?;
    Ok(())
}

// --- 公開ヘルパー ---

pub async fn get_feed_urls(
    db: &sqlx::SqlitePool,
    guild_id: &str,
    category: &str,
) -> Result<Vec<String>, Error> {
    let preset: Vec<String> = PRESET_FEEDS
        .iter()
        .filter(|f| f.category == category)
        .map(|f| f.url.to_string())
        .collect();

    if !preset.is_empty() {
        return Ok(preset);
    }

    let custom = sqlx::query_scalar::<_, String>(
        "SELECT url FROM custom_feeds WHERE guild_id = ? AND name = ?",
    )
    .bind(guild_id)
    .bind(category)
    .fetch_optional(db)
    .await?;

    Ok(custom.into_iter().collect())
}

pub async fn fetch_feeds(
    http_client: &reqwest::Client,
    urls: &[String],
    max_entries: usize,
) -> Vec<FeedEntry> {
    let mut all_entries = Vec::new();

    for url in urls {
        let Ok(resp) = http_client.get(url).send().await else {
            continue;
        };
        let Ok(body) = resp.bytes().await else {
            continue;
        };
        let Ok(feed) = feed_rs::parser::parse(&body[..]) else {
            continue;
        };

        for entry in feed.entries.into_iter().take(max_entries) {
            let title = entry
                .title
                .map(|t| t.content)
                .unwrap_or_else(|| "(無題)".to_string());
            let link = entry
                .links
                .first()
                .map(|l| l.href.clone())
                .unwrap_or_default();

            if !title.is_empty() {
                all_entries.push(FeedEntry { title, link });
            }
        }
    }

    all_entries.truncate(max_entries);
    all_entries
}

pub async fn generate_news_summary(
    http_client: &reqwest::Client,
    config: &crate::config::Config,
    titles: &str,
) -> String {
    let (api_url, api_key, model) = resolve_ai_config(config);
    tracing::debug!(provider = %config.bot.default_ai_provider, model = %model, url = %api_url, "news summary request");

    if api_key.is_empty() {
        tracing::error!("AI API key is empty for provider: {}", config.bot.default_ai_provider);
        return "（API キーが未設定のため要約を生成できません）".to_string();
    }

    let request = serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": format!(
                "以下のニュース見出しを日本語で簡潔に要約してください。3-5個の箇条書きで、それぞれ1文で。\n\n{}",
                titles
            )
        }]
    });

    let resp = http_client
        .post(&api_url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&request)
        .send()
        .await;

    let Ok(resp) = resp else {
        tracing::error!("news summary: HTTP request failed");
        return "（要約の生成に失敗しました: リクエスト送信エラー）".to_string();
    };

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if !status.is_success() {
        tracing::error!(status = %status, body = %body, "news summary: API returned error");
        return format!("（要約の生成に失敗: {} {}）", status, &body[..body.len().min(200)]);
    }

    let Ok(json) = serde_json::from_str::<serde_json::Value>(&body) else {
        tracing::error!(body = %&body[..body.len().min(500)], "news summary: failed to parse JSON");
        return "（要約の生成に失敗: レスポンス解析エラー）".to_string();
    };

    json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("（要約の生成に失敗: 応答なし）")
        .to_string()
}

fn resolve_ai_config(config: &crate::config::Config) -> (String, String, String) {
    match config.bot.default_ai_provider.as_str() {
        "openai" => (
            "https://api.openai.com/v1/chat/completions".to_string(),
            std::env::var("OPENAI_API_KEY").unwrap_or_default(),
            config.bot.default_model.clone(),
        ),
        "anthropic" => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
            config.bot.default_model.clone(),
        ),
        _ => (
            "https://openrouter.ai/api/v1/chat/completions".to_string(),
            std::env::var("OPENROUTER_API_KEY").unwrap_or_default(),
            config.bot.default_model.clone(),
        ),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(max).collect::<String>())
    }
}
