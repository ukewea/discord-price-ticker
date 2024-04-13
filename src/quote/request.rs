use crate::quote::error::QuoteRequestError;
use crate::quote::response::AssetQuoteResponse;
use std::result;
use std::sync::mpsc::Sender;

#[derive(Debug)]
pub struct AssetQuoteRequest {
    pub name: String,
    pub vs_currency: String,
    pub resp_sender: Sender<result::Result<AssetQuoteResponse, QuoteRequestError>>,
}
