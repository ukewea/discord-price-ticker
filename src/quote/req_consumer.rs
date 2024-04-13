use std::sync::mpsc::Receiver;
use std::thread::sleep;
use std::time::Duration;
use std::thread;
use bigdecimal::BigDecimal;
use reqwest::header;
use crate::quote::response::AssetQuoteResponse;
use crate::quote::error::QuoteRequestError;
use crate::quote::request::AssetQuoteRequest;
use std::str::FromStr;

pub fn consume_crypto_price_requests(job_receiver: Receiver<AssetQuoteRequest>, api_key: String) {
    macro_rules! sleep_then_continue {
        ($counter:expr) => {
            $counter -= 1;
            sleep(Duration::from_secs(1));
            continue;
        };
    }

    thread::spawn(move || {
        for req in job_receiver {
            let url: String = format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies={}&include_24hr_change=true", &req.name, &req.vs_currency);
            let mut retry_count = 3;
            let mut result = AssetQuoteResponse::dummy();
            let mut err: QuoteRequestError = QuoteRequestError::Other("No Error".to_string());
            let http_client = reqwest::blocking::Client::new();

            while retry_count > 0 {
                println!(
                    "Consumer sending request for {} to CoinGecko API, retry count: {}",
                    &req.name, retry_count
                );

                let http_req = http_client
                    .get(&url)
                    .header(header::ACCEPT, "application/json")
                    .header("x-cg-demo-api-key", &api_key);

                let response = match http_req.send() {
                    Ok(response) => response,
                    Err(e) => {
                        println!(
                            "Error calling CoinGecko API to get price for {}: {}, retrying...",
                            &req.name, e
                        );
                        err = e.into();
                        sleep_then_continue!(retry_count);
                    }
                };

                // example response
                // {"bitcoin":{"usd":65761,"usd_24h_change":1.8841205093585678}}
                let price_json: serde_json::Value =
                    match serde_json::from_str(response.text().unwrap().as_str()) {
                        Ok(json) => json,
                        Err(e) => {
                            println!(
                                "Error parsing JSON response for {} using CoinGecko API: {}",
                                req.name, e
                            );
                            err = e.into();
                            sleep_then_continue!(retry_count);
                        }
                    };

                let price_json = price_json.as_object().unwrap();

                if !price_json.contains_key(&req.name)
                    || !price_json[&req.name].is_object()
                {
                    println!(
                        "Error parsing JSON response for {} using CoinGecko API: {}",
                        &req.name, "missing id of the crypto, or is not an object"
                    );
                    err = "missing id of the API response, or is not an object".into();
                    sleep_then_continue!(retry_count);
                }

                let price_json_target_symbol = price_json[&req.name].as_object().unwrap();

                if !price_json_target_symbol.contains_key("usd")
                    || !price_json_target_symbol.contains_key("usd_24h_change")
                {
                    println!(
                        "Error parsing JSON response for {} using CoinGecko API: {}",
                        &req.name, "missing usd and/or usd_24h_change field(s)"
                    );
                    err =
                        "missing `usd` and/or `usd_24h_change` field(s) in the API response".into();
                    sleep_then_continue!(retry_count);
                }

                let price_usd = match price_json_target_symbol["usd"].as_number() {
                    Some(value) => value,
                    None => {
                        println!(
                            "Error parsing USD price for {} using CoinGecko API: {}",
                            &req.name, "missing field"
                        );
                        err = "cannot parse USD price as Number from the API response".into();
                        sleep_then_continue!(retry_count);
                    }
                };

                let price_usd: BigDecimal = match BigDecimal::from_str(price_usd.as_str()) {
                    Ok(value) => value,
                    Err(error) => {
                        println!(
                            "Error parsing USD price as BigDecimal for {} using CoinGecko API: {}",
                            &req.name, error
                        );
                        err = error.into();
                        sleep_then_continue!(retry_count);
                    }
                };

                let price_change_24h = match price_json_target_symbol["usd_24h_change"].as_f64() {
                    Some(value) => value,
                    None => {
                        println!(
                            "Error parsing 24h change for {} using CoinGecko API: {}",
                            &req.name, "missing field"
                        );
                        err = "cannot parse 24h change as f64 from the API response".into();
                        sleep_then_continue!(retry_count);
                    }
                };

                result = AssetQuoteResponse {
                    name: req.name.clone(),
                    price_usd,
                    price_change_24h,
                };
                break;
            }

            if retry_count <= 0 {
                let _ = match req.resp_sender.send(Err(err)) {
                    Ok(_) => {}
                    Err(error) => println!(
                        "Error sending response to channel for {}: {}",
                        &req.name, error
                    ),
                };
            } else {
                let _ = match req.resp_sender.send(Ok(result)) {
                    Ok(_) => {}
                    Err(error) => println!(
                        "Error sending response to channel for {}: {}",
                        &req.name, error
                    ),
                };
            }
        }
    });
}
