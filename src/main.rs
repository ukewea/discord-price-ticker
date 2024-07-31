use bigdecimal;
use serde_json;
use tracing_subscriber::fmt::format;
use std::io::Result;
use std::time;
use tokio::fs;
use tokio::signal;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::debug;
use tracing::trace;
use tracing::warn;
use bigdecimal::RoundingMode;
use tracing::info;
use tracing::error;
use tracing::instrument;
use tracing::Level;
use tracing_subscriber;

mod quote;
mod discord;
mod bot_update;
mod config;

use crate::quote::req_consumer::consume_crypto_price_requests;
use crate::quote::request::AssetQuoteRequest;
use crate::bot_update::BotUpdateInfo;
use crate::discord::client::DiscordClient;
use crate::config::{Config, TickerConfig};


async fn read_config(file_path: &str) -> Result<Config> {
    let config_string = fs::read_to_string(file_path).await?;
    let config: Config = serde_json::from_str(config_string.as_str())?;
    Ok(config)
}

#[instrument(skip_all, fields(ticker = ticker_config.ticker))]
async fn run_periodic_crypto_fetch_job_loop(
    ticker_config: TickerConfig,
    mut stop_signal_recv: oneshot::Receiver<()>,
    job_sender: mpsc::UnboundedSender<AssetQuoteRequest>,
    bot_update_sender: mpsc::UnboundedSender<BotUpdateInfo>,
    discord_client: DiscordClient,
) {
    const VS_CURRENCY: &str = "usd";
    const VS_CURRENCY_SYMBOL_PREFIX: &str = "$";
    const VS_CURRENCY_SYMBOL_SUFFIX: &str = "";

    macro_rules! break_if_signaled {
        ($stop_signal_recv:expr) => {
            match $stop_signal_recv.try_recv() {
                Ok(_) => {
                    info!("Stop signal received for {}", ticker_config.ticker);
                    let _ = stop_signal_recv.await;
                    break;
                }
                Err(e) => {
                    if e != oneshot::error::TryRecvError::Empty {
                        warn!(
                            "Error receiving stop signal for {}: {}",
                            ticker_config.ticker, e
                        );
                        break;
                    }
                }
            }
        };
    }

    let tick_duration = time::Duration::from_secs(ticker_config.frequency);
    let (get_price_chan_sender, mut get_price_chan_receiver) = mpsc::unbounded_channel();

    loop {
        break_if_signaled!(&mut stop_signal_recv);

        debug!(
            "Timer ticked for {}, fetching price...",
            ticker_config.ticker
        );

        let id = &ticker_config.name;

        let crypto_price_request = AssetQuoteRequest {
            name: id.to_string(),
            vs_currency: VS_CURRENCY.to_string(),
            resp_sender: get_price_chan_sender.clone(),
        };

        if let Err(e) = job_sender.send(crypto_price_request) {
            tracing::error!(
                "cannot send crypto price request to channel for {}, stopping: {}",
                id, e
            );
            break;
        }

        let get_price_chan_response = match get_price_chan_receiver.recv().await {
            Some(r) => r,
            None => {
                warn!(
                    "get crypto price response channel of '{}' is closed, will retry if possible",
                    ticker_config.ticker
                );
                continue;
            }
        };

        let get_price_response = match get_price_chan_response {
            Ok(r) => r,
            Err(error) => {
                warn!(
                    "Error getting price for {}: {}",
                    ticker_config.ticker, error
                );
                tokio::time::sleep(tick_duration).await;
                continue;
            }
        };

        let price_usd = get_price_response.price_usd;
        let price_change_24h = get_price_response.price_change_24h;

        let formatted_price_usd = format_price(&price_usd, ticker_config.decimals);
        let formatted_price_change_24h = format_price_change(price_change_24h);

        debug!(
            "Price for {} is {} USD (original value: {}), change in 24h is {}%",
            ticker_config.ticker, formatted_price_usd, price_usd, formatted_price_change_24h
        );

        break_if_signaled!(&mut stop_signal_recv);

        let discord_bot_name = generate_discord_bot_name(formatted_price_usd.as_str(), VS_CURRENCY_SYMBOL_PREFIX, VS_CURRENCY_SYMBOL_SUFFIX);
        let discord_bot_status = generate_discord_bot_status(formatted_price_change_24h.as_str(), ticker_config.ticker.as_str());

        debug!(
            "Update Discord bot name for {}, set to {} ({})...",
            ticker_config.ticker, discord_bot_name, discord_bot_status
        );

        if let Err(e) = bot_update_sender.send(BotUpdateInfo {
            name: discord_bot_name,
            status: discord_bot_status,
            discord_client: discord_client.clone(),
        }) {
            warn!("Failed to send bot update: {}", e);
        }

        break_if_signaled!(&mut stop_signal_recv);

        if let Ok(_) = timeout(tick_duration, &mut stop_signal_recv).await {
            info!(
                "Received stop signal for {}, quit loop",
                ticker_config.ticker
            );
            break;
        }
    }
}

fn format_price(price: &bigdecimal::BigDecimal, decimals: u8) -> String {
    if price.fractional_digit_count() > decimals as i64 {
        return price
           .with_scale_round(decimals.into(), RoundingMode::HalfEven)
           .to_string();
    } else {
        return price.to_string();
    }
}

fn format_price_change(price_change: f64) -> String {
    // if price change > 0, add a plus sign
    if price_change >= 0.0 {
        return format!("+{:.*}%", 2, price_change)
    }

    format!("{:.*}%", 2, price_change)
}

fn generate_discord_bot_name(
    formatted_price: &str,
    vs_currency_symbol_prefix: &str,
    vs_currency_symbol_suffix: &str,
) -> String {
    if vs_currency_symbol_suffix.is_empty() {
        return format!("{}{}", vs_currency_symbol_prefix, formatted_price);
    }

    format!(
        "{}{} {}",
        vs_currency_symbol_prefix, formatted_price, vs_currency_symbol_suffix
    )
}

fn generate_discord_bot_status(formatted_price_change: &str, ticker: &str) -> String {
    format!("{}% | {}", formatted_price_change, ticker)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("Hello, world!");

    let config = match read_config("app_config.json").await {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("Error reading config file: {}", error);
            return;
        }
    };

    let (crypto_price_req_sender, crypto_price_req_receiver) = mpsc::unbounded_channel();
    // let (stock_price_req_sender, stock_price_req_receiver) = mpsc::unbounded_channel();
    let (bot_update_sender, mut bot_update_receiver) = mpsc::unbounded_channel();

    let mut tasks = Vec::new();
    let mut stop_signal_channels = Vec::new();

    for ticker_config in config.tickers {
        debug!(
            "Loaded config for ticker: {}, is crypto? {}",
            ticker_config.ticker, ticker_config.crypto
        );

        if !is_bot_token_valid(&ticker_config.discord_bot_token) {
            error!(
                "Invalid Discord bot token for ticker {}, skipping",
                ticker_config.ticker
            );
            continue;
        }

        let (stop_signal_send, stop_signal_recv) = oneshot::channel();
        let discord_client = DiscordClient::new(&ticker_config.discord_bot_token).await;
        let ticker = ticker_config.ticker.to_string();

        if ticker_config.crypto {
            let crypto_price_req_sender_clone = crypto_price_req_sender.clone();
            let bot_update_sender_clone = bot_update_sender.clone();

            trace!("Spawning task for crypto ticker: {}", ticker);
            tasks.push(tokio::spawn(async move {
                run_periodic_crypto_fetch_job_loop(
                    ticker_config,
                    stop_signal_recv,
                    crypto_price_req_sender_clone,
                    bot_update_sender_clone,
                    discord_client,
                )
                .await;
            }));

            trace!("Creating stop signal channel for ticker: {}", ticker);
        } else {
            // TODO: implement stock price fetching
            // spawn_job(ticker_config, Arc::clone(&stop_flag), stock_price_req_sender.clone());
            warn!(
                "TBD: ticker {} is not an US stock, not implemented yet, skip",
                ticker_config.ticker
            )
        }

        stop_signal_channels.push((ticker, stop_signal_send));
    }

    let coingecko_api_key = config.coingecko_api_key.to_string();
    trace!("Starting crypto price request consumer...");
    tokio::spawn(async move {
        consume_crypto_price_requests(crypto_price_req_receiver, coingecko_api_key).await;
    });

    tokio::spawn(async move {
        while let Some(update) = bot_update_receiver.recv().await {
            update.discord_client.update_bot(update.name, update.status).await;
        }
    });

    // consume_stock_price_requests(stock_price_req_receiver, ...);

    trace!("Starting signal handler...");
    tokio::spawn(async move {
        trace!("Waiting for Ctrl+C signal...");
        signal::ctrl_c().await.expect("failed to listen for event");

        info!("Ctrl+C pressed. Stopping...");
        for (ticker, stop_signal) in stop_signal_channels {
            info!("Sending stop signal to receiver for ticker: {}", ticker);
            if let Err(_) = stop_signal.send(()) {
                warn!("Stop signal receiver for ticker {} is already dropped", ticker);
            } else {
                info!("Stop signal sent to receiver for ticker: {}", ticker);
            }
        }
    });

    info!("Waiting for all tasks to finish...");
    for task in tasks {
        let _ = task.await;
    }

    info!("All tasks finished.");
}

fn is_bot_token_valid(bot_token: &str) -> bool {
    // just check if the token is empty
    !bot_token.trim().is_empty() && bot_token.is_ascii()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_format_price() {
        let price = bigdecimal::BigDecimal::from_str("123456789.123456789").unwrap();
        assert_eq!("123456789", format_price(&price, 0));
        assert_eq!("123456789.12", format_price(&price, 2));
        assert_eq!("123456789.123456789", format_price(&price, 9));
        assert_eq!("123456789.123456789", format_price(&price, 10));
    }

    #[test]
    fn test_format_price_change() {
        assert_eq!("+12.34%", format_price_change(12.34));
        assert_eq!("-0.12%", format_price_change(-0.12));
        assert_eq!("+0.00%", format_price_change(0.0));
    }

    #[test]
    fn test_generate_discord_bot_name() {
        assert_eq!("$1234.56", generate_discord_bot_name("1234.56", "$", ""));
        assert_eq!("$1234.56   space  ", generate_discord_bot_name("1234.56", "$", "  space  "));
        assert_eq!("$1234.56 USD", generate_discord_bot_name("1234.56", "$", "USD"));
        assert_eq!("1234.56 USD", generate_discord_bot_name("1234.56", "", "USD"));
        assert_eq!("1234.56", generate_discord_bot_name("1234.56", "", ""));
    }

    #[test]
    fn test_generate_discord_bot_status() {
        assert_eq!("+12.34% | TICKER", generate_discord_bot_status("+12.34", "TICKER"));
        assert_eq!("-0.12% | TICKER", generate_discord_bot_status("-0.12", "TICKER"));
        assert_eq!("+0.00% | TICKER", generate_discord_bot_status("+0.00", "TICKER"));
    }
}
