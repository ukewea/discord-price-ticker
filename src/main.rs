use crate::quote::req_consumer::consume_crypto_price_requests;
use crate::quote::request::AssetQuoteRequest;
use serde::Deserialize;
use serde_json;
use std::fs;
use std::io::Result;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time;

mod quote;

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

fn read_config(file_path: &str) -> Result<Config> {
    let config_string = fs::read_to_string(file_path)?;
    let config: Config = serde_json::from_str(config_string.as_str())?;
    Ok(config)
}

fn spawn_job(
    ticker_config: TickerConfig,
    stop_flag: Arc<Mutex<bool>>,
    job_sender: Sender<AssetQuoteRequest>,
) {
    const VS_CURRENCY: &str = "usd";
    const VS_CURRENCY_SYMBOL_PREFIX: &str = "$";
    const VS_CURRENCY_SYMBOL_SUFFIX: &str = "";

    macro_rules! check_and_break {
        ($stop_flag:expr) => {
            if *$stop_flag.lock().unwrap() {
                break;
            }
        };
    }

    thread::spawn(move || {
        let tick_duration = time::Duration::from_secs(ticker_config.frequency);
        let (get_price_chan_sender, get_price_chan_receiver) = mpsc::channel();

        loop {
            check_and_break!(stop_flag);

            let formatted_price_usd: String;
            let formatted_price_change_24h: String;

            println!(
                "Timer ticked for {}, fetching price...",
                ticker_config.ticker
            );

            let id = &ticker_config.name;

            let crypto_price_request = AssetQuoteRequest {
                name: id.to_string(),
                vs_currency: VS_CURRENCY.to_string(),
                resp_sender: get_price_chan_sender.clone(),
            };
            job_sender.send(crypto_price_request).unwrap();

            let get_price_chan_response = match get_price_chan_receiver.recv() {
                Ok(r) => r,
                Err(error) => {
                    println!(
                        "Error receiving response from channel for {}: {}",
                        ticker_config.ticker, error
                    );
                    thread::sleep(tick_duration);
                    continue;
                }
            };

            let get_price_response = match get_price_chan_response {
                Ok(r) => r,
                Err(error) => {
                    println!(
                        "Error getting price for {}: {}",
                        ticker_config.ticker, error
                    );
                    thread::sleep(tick_duration);
                    continue;
                }
            };

            let price_usd = get_price_response.price_usd;
            let price_change_24h = get_price_response.price_change_24h;

            formatted_price_usd = format!("{:.*}", ticker_config.decimals as usize, price_usd);
            formatted_price_change_24h = format!("{:.*}", 2, price_change_24h);
            println!(
                "Price for {} is {} USD, change in 24h is {}%",
                ticker_config.ticker, formatted_price_usd, formatted_price_change_24h
            );

            check_and_break!(stop_flag);

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

            println!(
                "Update Discord bot name for {}, set to {} ({})...",
                ticker_config.ticker, discord_bot_name, discord_bot_status
            );
            // TODO: update Discord bot name

            check_and_break!(stop_flag);

            thread::sleep(tick_duration);
        }
    });
}

fn main() {
    println!("Hello, world!");
    let read_result = read_config("app_config.json");

    if let Ok(config) = read_result {
        let stop_flag = Arc::new(Mutex::new(false));
        let (crypto_price_req_sender, crypto_price_req_receiver) = mpsc::channel();
        // let (stock_price_req_sender, stock_price_req_receiver) = mpsc::channel();

        for ticker_config in config.tickers {
            println!("Loaded config for ticker: {}", ticker_config.ticker);
            if ticker_config.crypto {
                spawn_job(ticker_config, Arc::clone(&stop_flag), crypto_price_req_sender.clone());
            } else {
                // TODO: implement stock price fetching
                // spawn_job(ticker_config, Arc::clone(&stop_flag), stock_price_req_sender.clone());
                println!("TBD: stock price fetching not implemented yet")
            }
        }

        consume_crypto_price_requests(crypto_price_req_receiver, config.coingecko_api_key);
        // consume_stock_price_requests(stock_price_req_receiver, ...);

        // blockingly wait for all threads to finish
        println!("Waiting for all threads to finish...");
        thread::park();
    } else {
        println!("Error reading config file: {}", read_result.unwrap_err());
    }
}
