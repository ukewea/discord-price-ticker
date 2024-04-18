use crate::quote::error::QuoteRequestError;
use crate::quote::response::AssetQuoteResponse;
use std::result;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug)]
pub struct AssetQuoteRequest {
    pub name: String,
    pub vs_currency: String,
    pub resp_sender: UnboundedSender<result::Result<AssetQuoteResponse, QuoteRequestError>>,
}
