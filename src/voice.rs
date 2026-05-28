use poise::serenity_prelude as serenity;
use songbird::{input::File, Event, EventContext, EventHandler, TrackEvent};
use std::sync::Arc;

pub async fn play_sound_in_vc(
    manager: &Arc<songbird::Songbird>,
    guild_id: serenity::GuildId,
    channel_id: serenity::ChannelId,
    audio_path: &str,
    auto_leave_timeout: std::time::Duration,
) -> Result<(), crate::error::Error> {
    let handler_lock = manager.join(guild_id, channel_id).await?;

    let mut handler = handler_lock.lock().await;
    let source = File::new(audio_path.to_string());
    let track_handle = handler.play_input(source.into());

    let manager_clone = manager.clone();
    track_handle.add_event(
        Event::Track(TrackEvent::End),
        TrackEndLeaver {
            manager: manager_clone,
            guild_id,
            timeout: auto_leave_timeout,
        },
    )?;

    Ok(())
}

pub async fn join_vc(
    manager: &Arc<songbird::Songbird>,
    guild_id: serenity::GuildId,
    channel_id: serenity::ChannelId,
) -> Result<Arc<tokio::sync::Mutex<songbird::Call>>, crate::error::Error> {
    match manager.join(guild_id, channel_id).await {
        Ok(handler) => {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            return Ok(handler);
        }
        Err(e) => {
            tracing::warn!("VC join failed: {:?}", e);
        }
    }

    // join() failed but voice state update was sent (bot appears in VC).
    // The UDP connection may still be establishing — grab the existing handler.
    if let Some(handler) = manager.get(guild_id) {
        tracing::warn!("Using existing handler after join failure, waiting 5s for connection...");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        let guard = handler.lock().await;
        let conn = guard.current_connection();
        tracing::warn!("Connection state after wait: {:?}", conn);
        drop(guard);
        return Ok(handler);
    }

    // No handler at all — clean up and retry once
    manager.leave(guild_id).await.ok();
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    let handler = manager.join(guild_id, channel_id).await?;
    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    Ok(handler)
}

pub async fn play_on_handler(
    handler_lock: &Arc<tokio::sync::Mutex<songbird::Call>>,
    audio_path: &str,
    volume: f32,
) -> Result<(), crate::error::Error> {
    if !std::path::Path::new(audio_path).exists() {
        return Err(format!("音声ファイルが見つかりません: {}", audio_path).into());
    }

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut handler = handler_lock.lock().await;
        let info = handler.current_connection();
        tracing::warn!("VC connection before play: {:?}", info);

        let source = File::new(audio_path.to_string());
        let track_handle = handler.play_input(source.into());
        let _ = track_handle.set_volume(volume);

        track_handle.add_event(
            Event::Track(TrackEvent::End),
            TrackEndNotifier {
                sender: std::sync::Mutex::new(Some(tx)),
            },
        )?;
    }

    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(_) => Ok(()),
        Err(_) => {
            tracing::warn!("play_on_handler timed out after 30s — VC connection may not be established");
            Err("音声再生がタイムアウトしました（VC接続未確立の可能性）".into())
        }
    }
}

pub async fn leave_vc(manager: &Arc<songbird::Songbird>, guild_id: serenity::GuildId) {
    manager.leave(guild_id).await.ok();
}

struct TrackEndLeaver {
    manager: Arc<songbird::Songbird>,
    guild_id: serenity::GuildId,
    timeout: std::time::Duration,
}

#[async_trait::async_trait]
impl EventHandler for TrackEndLeaver {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        let manager = self.manager.clone();
        let guild_id = self.guild_id;
        let timeout = self.timeout;

        tokio::spawn(async move {
            tokio::time::sleep(timeout).await;
            manager.leave(guild_id).await.ok();
        });

        None
    }
}

struct TrackEndNotifier {
    sender: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>,
}

#[async_trait::async_trait]
impl EventHandler for TrackEndNotifier {
    async fn act(&self, _ctx: &EventContext<'_>) -> Option<Event> {
        if let Ok(mut guard) = self.sender.lock() {
            if let Some(tx) = guard.take() {
                let _ = tx.send(());
            }
        }
        None
    }
}
