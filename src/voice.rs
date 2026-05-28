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
