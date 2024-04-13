use bigdecimal::BigDecimal;
use std::str::FromStr;

#[derive(Debug)]
pub struct AssetQuoteResponse {
    pub name: String,
    pub price_usd: BigDecimal,
    pub price_change_24h: f64,
}

impl AssetQuoteResponse {
    pub fn dummy() -> Self {
        Self {
            name: "dummy".to_string(),
            price_usd: BigDecimal::from_str("-99999.00").unwrap(),
            price_change_24h: -999.0,
        }
    }
}
