mod commands;
mod config;
mod db;
mod error;

use poise::serenity_prelude as serenity;
use songbird::SerenityInit;
use sqlx::SqlitePool;

pub struct Data {
    pub db: SqlitePool,
    pub http_client: reqwest::Client,
    pub config: config::Config,
}

#[tokio::main]
async fn main() -> error::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let token = std::env::var("DISCORD_TOKEN").expect("DISCORD_TOKEN not set");

    let config = config::Config::load("config.toml").unwrap_or_default();
    let pool = db::init(&config.database.path).await?;
    let http_client = reqwest::Client::new();

    let pool_bg = pool.clone();

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::ask::ask(),
                commands::todo::todo(),
                commands::diary::diary(),
                commands::pomo::pomo(),
                commands::remind::remind(),
                commands::habit::habit(),
                commands::alarm::alarm(),
            ],
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("quid-bot is ready!");

                let http = ctx.http.clone();
                tokio::spawn(async move {
                    commands::remind::reminder_loop(http, pool_bg.clone()).await;
                });

                Ok(Data {
                    db: pool,
                    http_client,
                    config,
                })
            })
        })
        .build();

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT;

    let mut client = serenity::ClientBuilder::new(token, intents)
        .framework(framework)
        .register_songbird()
        .await?;

    client.start().await?;
    Ok(())
}
