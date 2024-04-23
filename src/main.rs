use crate::quote::req_consumer::consume_crypto_price_requests;
use crate::quote::request::AssetQuoteRequest;
use bigdecimal;
use serde::Deserialize;
use serde_json;
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
mod quote;
use bigdecimal::RoundingMode;
use tracing::info;
use tracing::instrument;
use tracing::Level;
use tracing_subscriber;

#[derive(Debug, Deserialize)]
struct Config {
    coingecko_api_key: String, // Field to store the CoinGecko API key for fetching crypto prices
    tickers: Vec<TickerConfig>, // Field to store the list of ticker configurations
}

#[derive(Debug, Deserialize)]
struct TickerConfig {
    ticker: String, // Ticker symbol, it will be displayed as status of the Discord bot
    name: String, // Field to store the name of the ticker, if it is crypto, it will be the id in CoinGecko API
    crypto: bool, // Field to represent whether the ticker is related to cryptocurrency
    frequency: u64, // Field to store the frequency of updates, in seconds
    decimals: u8, // Field to store the number of decimal places for the ticker value
    discord_bot_token: String, // Field to store the Discord bot token for authentication
}

async fn read_config(file_path: &str) -> Result<Config> {
    let config_string = fs::read_to_string(file_path).await?;
    let config: Config = serde_json::from_str(config_string.as_str())?;
    Ok(config)
}

#[instrument(skip_all, fields(ticker = ticker_config.ticker))]
async fn run_periodic_job_loop(
    ticker_config: TickerConfig,
    mut stop_signal_recv: oneshot::Receiver<()>,
    job_sender: mpsc::UnboundedSender<AssetQuoteRequest>,
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

        let formatted_price_usd: String;
        let formatted_price_change_24h: String;

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

        if price_usd.fractional_digit_count() > ticker_config.decimals as i64 {
            formatted_price_usd = price_usd
                .with_scale_round(ticker_config.decimals.into(), RoundingMode::HalfEven)
                .to_string();
        } else {
            formatted_price_usd = price_usd.to_string();
        }

        formatted_price_change_24h = format!("{:.*}", 2, price_change_24h);
        debug!(
            "Price for {} is {} USD (original value: {}), change in 24h is {}%",
            ticker_config.ticker, formatted_price_usd, price_usd, formatted_price_change_24h
        );

        break_if_signaled!(&mut stop_signal_recv);

        let discord_bot_name: String;
        if VS_CURRENCY_SYMBOL_SUFFIX.is_empty() {
            discord_bot_name = format!("{}{}", VS_CURRENCY_SYMBOL_PREFIX, formatted_price_usd);
        } else {
            discord_bot_name = format!(
                "{}{} {}",
                VS_CURRENCY_SYMBOL_PREFIX, formatted_price_usd, VS_CURRENCY_SYMBOL_SUFFIX
            );
        }

        let discord_bot_status =
            format!("{}% | {}", formatted_price_change_24h, ticker_config.ticker);

        debug!(
            "Update Discord bot name for {}, set to {} ({})...",
            ticker_config.ticker, discord_bot_name, discord_bot_status
        );
        // TODO: update Discord bot name

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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_max_level(Level::DEBUG)
        .init();

    info!("Hello, world!");
    let read_result = read_config("app_config.json").await;

    let config = match read_result {
        Ok(config) => config,
        Err(error) => {
            tracing::error!("Error reading config file: {}", error);
            return;
        }
    };

    let (crypto_price_req_sender, crypto_price_req_receiver) = mpsc::unbounded_channel();
    // let (stock_price_req_sender, stock_price_req_receiver) = mpsc::unbounded_channel();

    let mut tasks = Vec::new();
    let mut stop_signal_channels = Vec::new();

    for ticker_config in config.tickers {
        debug!(
            "Loaded config for ticker: {}, is crypto? {}",
            ticker_config.ticker, ticker_config.crypto
        );
        if ticker_config.crypto {
            let ticker = ticker_config.ticker.to_string();
            let crypto_price_req_sender_clone = crypto_price_req_sender.clone();
            let (stop_signal_send, stop_signal_recv) = oneshot::channel();

            trace!("Spawning task for ticker: {}", ticker);
            tasks.push(tokio::spawn(async move {
                run_periodic_job_loop(
                    ticker_config,
                    stop_signal_recv,
                    crypto_price_req_sender_clone,
                )
                .await;
            }));

            trace!("Creating stop signal channel for ticker: {}", ticker);
            stop_signal_channels.push((ticker, stop_signal_send));
        } else {
            // TODO: implement stock price fetching
            // spawn_job(ticker_config, Arc::clone(&stop_flag), stock_price_req_sender.clone());
            warn!(
                "TBD: ticker {} is not an US stock, not implemented yet",
                ticker_config.ticker
            )
        }
    }

    let coingecko_api_key = config.coingecko_api_key.to_string();
    trace!("Starting crypto price request consumer...");
    tokio::spawn(async move {
        consume_crypto_price_requests(crypto_price_req_receiver, coingecko_api_key).await;
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
