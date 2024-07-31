use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub coingecko_api_key: String, // Field to store the CoinGecko API key for fetching crypto prices
    pub tickers: Vec<TickerConfig>, // Field to store the list of ticker configurations
}

#[derive(Debug, Deserialize)]
pub struct TickerConfig {
    pub ticker: String, // Ticker symbol, it will be displayed as status of the Discord bot
    pub name: String, // Field to store the name of the ticker, if it is crypto, it will be the id in CoinGecko API
    pub crypto: bool, // Field to represent whether the ticker is related to cryptocurrency
    pub frequency: u64, // Field to store the frequency of updates, in seconds
    pub decimals: u8, // Field to store the number of decimal places for the ticker value
    pub discord_bot_token: String, // Field to store the Discord bot token for authentication
}