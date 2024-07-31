use serenity::all::{ActivityData, GuildId, GuildInfo, GuildPagination, Http};
use serenity::prelude::*;
use tracing::{debug, trace, warn};
use std::sync::Arc;
use serenity::gateway::ShardManager;
use tracing::error;

#[derive(Clone, Debug)]
pub struct DiscordClient {
    http_client: Arc<Http>,
    shard_manager: Arc<ShardManager>,
}

impl DiscordClient {
    pub async fn new(token: &str) -> Self {
        let intents = GatewayIntents::default();
        let mut client = Client::builder(&token, intents)
            .await
            .expect("Err creating client");
        let shard_manager = client.shard_manager.clone();
        let http_client = client.http.clone();

        tokio::spawn(async move {
            // Start two shards. Note that there is an ~5 second ratelimit period between when one shard
            // can start after another.
            if let Err(why) = client.start_shards(1).await {
                error!("Client error: {why:?}");
            }
        });

        Self { http_client: http_client, shard_manager }
    }

    pub async fn update_bot(&self, name: String, status: String) {
        let guilds = match self.get_guilds().await {
            Ok(guilds) => guilds,
            Err(why) => {
                warn!("Error getting guilds: {why:?}");
                return;
            }
        };
        debug!("Update nicknames in guilds:");
        for g in &guilds {
            // let roles = match self.http_client.get_guild_roles(g.id).await {
            //     Ok(roles) => roles,
            //     Err(why) => {
            //         warn!("  Error getting roles for guild {}: {why:?}", g.name);
            //         return;
            //     }
            // };

            // for role in roles {
            //     debug!("  Role: {}, id = {}", role.name, role.id);
            // }

            match self.http_client.edit_nickname(g.id, Some(&name), None).await {
                Ok(_) => debug!("Updated nickname for guild {} to {}", g.name, name),
                Err(why) => warn!("Error updating nickname for guild {}: {why:?}", g.name),
            };
        }

        // Update bot's activity
        let shard_runners = self.shard_manager.runners.lock().await;
        for (shard_id, runner) in shard_runners.iter() {
            let new_activity = ActivityData::custom(&status);
            runner.runner_tx.set_activity(Some(new_activity));
            debug!(
                "Updated activity for shard {} to {}",
                shard_id, status
            );
        }
    }

    async fn get_guilds(&self) -> Result<Vec<GuildInfo>, SerenityError> {
        let mut guilds = Vec::<GuildInfo>::new();
        let mut retry_count = 3;
        let mut last_id: Option<GuildId> = None;
        let limit = 100;

        loop {
            let target = last_id.map(GuildPagination::After);

            match self.http_client.get_guilds(target, Some(limit)).await {
                Ok(guilds_partial) => {
                    let partial_len = guilds_partial.len();
                    guilds.extend(guilds_partial);
                    if partial_len >= limit as usize {
                        trace!("Got {} guilds, continue", limit);
                        last_id = Some(guilds.last().unwrap().id);
                    } else {
                        trace!("Got all guilds, break");
                        break;
                    }
                }
                Err(why) => {
                    tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                    retry_count -= 1;
                    if retry_count == 0 {
                        return Err(why);
                    }
                    continue;
                }
            };
        }

        Ok(guilds)
    }
}