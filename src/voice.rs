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
    if let Ok(handler) = manager.join(guild_id, channel_id).await {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        return Ok(handler);
    }

    // join() failed but voice state update was sent (bot appears in VC).
    // The UDP connection may still be establishing — grab the existing handler.
    if let Some(handler) = manager.get(guild_id) {
        tracing::info!("VC join timed out, using existing handler (connection may be in progress)");
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
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
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    {
        let mut handler = handler_lock.lock().await;
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

    let _ = rx.await;
    Ok(())
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
