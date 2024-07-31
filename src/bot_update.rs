use crate::discord::client::DiscordClient;

#[derive(Clone, Debug)]
pub struct BotUpdateInfo {
    pub name: String,
    pub status: String,
    pub discord_client: DiscordClient,
}