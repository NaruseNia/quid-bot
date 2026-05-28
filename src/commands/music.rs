use super::{Context, Error};
use poise::serenity_prelude::CreateEmbed;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;

static SPOTIFY_TOKEN: std::sync::LazyLock<Arc<Mutex<Option<SpotifyToken>>>> =
    std::sync::LazyLock::new(|| Arc::new(Mutex::new(None)));

struct SpotifyToken {
    access_token: String,
    expires_at: std::time::Instant,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Deserialize)]
struct SearchResponse {
    tracks: TrackItems,
}

#[derive(Deserialize)]
struct TrackItems {
    items: Vec<Track>,
    total: u64,
}

#[derive(Deserialize)]
struct Track {
    name: String,
    artists: Vec<Artist>,
    album: Album,
    external_urls: ExternalUrls,
    preview_url: Option<String>,
    duration_ms: u64,
}

#[derive(Deserialize)]
struct Artist {
    name: String,
}

#[derive(Deserialize)]
struct Album {
    name: String,
    images: Vec<Image>,
    release_date: Option<String>,
}

#[derive(Deserialize)]
struct Image {
    url: String,
}

#[derive(Deserialize)]
struct ExternalUrls {
    spotify: Option<String>,
}

#[derive(Deserialize)]
struct RecommendationsResponse {
    tracks: Vec<Track>,
}

const GENRE_SEEDS: &[&str] = &[
    "pop", "rock", "hip-hop", "jazz", "classical", "electronic", "r-n-b",
    "indie", "metal", "punk", "blues", "soul", "funk", "reggae", "country",
    "latin", "k-pop", "j-pop", "anime", "ambient", "house", "techno",
    "drum-and-bass", "lo-fi", "chill",
];

/// 音楽
#[poise::command(slash_command, subcommands("random", "genres"))]
pub async fn music(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

/// ランダムに1曲おすすめ
#[poise::command(slash_command)]
async fn random(
    ctx: Context<'_>,
    #[description = "ジャンル (例: rock, jazz, k-pop, anime)"] genre: Option<String>,
    #[description = "検索キーワード"] keyword: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;

    let data = ctx.data();
    let token = get_spotify_token(&data.http_client).await?;

    let track = if let Some(ref kw) = keyword {
        search_random_track(&data.http_client, &token, kw).await?
    } else {
        let genre = genre
            .as_deref()
            .unwrap_or(GENRE_SEEDS[rand_index(GENRE_SEEDS.len())]);
        recommend_random_track(&data.http_client, &token, genre).await?
    };

    let Some(track) = track else {
        ctx.send(poise::CreateReply::default().embed(
            CreateEmbed::new()
                .description("曲が見つかりませんでした。別のジャンルやキーワードを試してみてください。")
                .color(0xED4245),
        )).await?;
        return Ok(());
    };

    let artists = track
        .artists
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    let duration = format!("{}:{:02}", track.duration_ms / 60000, (track.duration_ms / 1000) % 60);

    let spotify_url = track
        .external_urls
        .spotify
        .as_deref()
        .unwrap_or("");

    let mut embed = CreateEmbed::new()
        .title(&track.name)
        .url(spotify_url)
        .color(0x1DB954)
        .field("アーティスト", &artists, true)
        .field("アルバム", &track.album.name, true)
        .field("再生時間", &duration, true);

    if let Some(ref date) = track.album.release_date {
        embed = embed.field("リリース", date, true);
    }

    if let Some(img) = track.album.images.first() {
        embed = embed.thumbnail(&img.url);
    }

    if let Some(ref preview) = track.preview_url {
        embed = embed.field("プレビュー", format!("[30秒試聴]({})", preview), false);
    }

    embed = embed.footer(poise::serenity_prelude::CreateEmbedFooter::new("Spotify"));

    ctx.send(poise::CreateReply::default().embed(embed)).await?;
    Ok(())
}

/// 使用可能なジャンル一覧
#[poise::command(slash_command)]
async fn genres(ctx: Context<'_>) -> Result<(), Error> {
    let list = GENRE_SEEDS
        .iter()
        .map(|g| format!("`{}`", g))
        .collect::<Vec<_>>()
        .join("  ");

    ctx.send(
        poise::CreateReply::default().embed(
            CreateEmbed::new()
                .title("🎵 ジャンル一覧")
                .description(list)
                .color(0x1DB954),
        ),
    )
    .await?;
    Ok(())
}

// --- Spotify API helpers ---

async fn get_spotify_token(http_client: &reqwest::Client) -> Result<String, Error> {
    {
        let guard = SPOTIFY_TOKEN.lock().await;
        if let Some(ref t) = *guard
            && t.expires_at > std::time::Instant::now()
        {
            return Ok(t.access_token.clone());
        }
    }

    let client_id = std::env::var("SPOTIFY_CLIENT_ID")
        .map_err(|_| "SPOTIFY_CLIENT_ID not set")?;
    let client_secret = std::env::var("SPOTIFY_CLIENT_SECRET")
        .map_err(|_| "SPOTIFY_CLIENT_SECRET not set")?;

    let resp = http_client
        .post("https://accounts.spotify.com/api/token")
        .basic_auth(&client_id, Some(&client_secret))
        .form(&[("grant_type", "client_credentials")])
        .send()
        .await?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Spotify auth failed: {}", body).into());
    }

    let token_resp: TokenResponse = resp.json().await?;
    let access_token = token_resp.access_token.clone();

    let mut guard = SPOTIFY_TOKEN.lock().await;
    *guard = Some(SpotifyToken {
        access_token: token_resp.access_token,
        expires_at: std::time::Instant::now()
            + std::time::Duration::from_secs(token_resp.expires_in.saturating_sub(60)),
    });

    Ok(access_token)
}

async fn recommend_random_track(
    http_client: &reqwest::Client,
    token: &str,
    genre: &str,
) -> Result<Option<Track>, Error> {
    let url = format!(
        "https://api.spotify.com/v1/recommendations?seed_genres={}&limit=20",
        genre
    );

    let resp = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?;

    if !resp.status().is_success() {
        let fallback_query = format!("genre:{}", genre);
        return search_random_track(http_client, token, &fallback_query).await;
    }

    let data: RecommendationsResponse = resp.json().await?;
    if data.tracks.is_empty() {
        return Ok(None);
    }

    let idx = rand_index(data.tracks.len());
    Ok(data.tracks.into_iter().nth(idx))
}

async fn search_random_track(
    http_client: &reqwest::Client,
    token: &str,
    query: &str,
) -> Result<Option<Track>, Error> {
    let first_url = format!(
        "https://api.spotify.com/v1/search?q={}&type=track&limit=1",
        urlencod(query)
    );
    let first_resp = http_client
        .get(&first_url)
        .bearer_auth(token)
        .send()
        .await?;

    if !first_resp.status().is_success() {
        return Ok(None);
    }

    let first_data: SearchResponse = first_resp.json().await?;
    let total = first_data.tracks.total.min(1000);
    if total == 0 {
        return Ok(None);
    }

    let offset = rand_index(total as usize);
    let url = format!(
        "https://api.spotify.com/v1/search?q={}&type=track&limit=1&offset={}",
        urlencod(query),
        offset
    );

    let resp = http_client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let data: SearchResponse = resp.json().await?;
    Ok(data.tracks.items.into_iter().next())
}

fn rand_index(max: usize) -> usize {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    std::time::SystemTime::now().hash(&mut hasher);
    std::thread::current().id().hash(&mut hasher);
    (hasher.finish() as usize) % max
}

fn urlencod(s: &str) -> String {
    s.replace(' ', "+")
        .replace('&', "%26")
        .replace('#', "%23")
}
