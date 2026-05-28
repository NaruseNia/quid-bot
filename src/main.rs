mod commands;
mod config;
mod db;
mod error;
mod voice;

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

    let pool_remind = pool.clone();
    let pool_alarm = pool.clone();
    let config_alarm = config.clone();

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
            event_handler: |_ctx, event, _framework, _data| {
                Box::pin(async move {
                    if let serenity::FullEvent::Message { new_message } = event {
                        if new_message.author.bot {
                            return Ok(());
                        }
                        commands::diary::handle_message(
                            new_message.author.id,
                            new_message.channel_id,
                            new_message.guild_id,
                            &new_message.content,
                        )
                        .await;
                    }
                    Ok(())
                })
            },
            ..Default::default()
        })
        .setup(move |ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;
                tracing::info!("quid-bot is ready!");

                let http = ctx.http.clone();
                tokio::spawn(async move {
                    commands::remind::reminder_loop(http, pool_remind).await;
                });

                if let Some(manager) = songbird::get(ctx).await {
                    let http2 = ctx.http.clone();
                    let alarm_file = config_alarm.audio.alarm_file.clone();
                    let auto_leave = std::time::Duration::from_secs(
                        config_alarm.audio.auto_leave_timeout_sec,
                    );
                    tokio::spawn(async move {
                        commands::alarm::alarm_loop(
                            http2,
                            pool_alarm,
                            manager,
                            alarm_file,
                            auto_leave,
                        )
                        .await;
                    });
                }

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
