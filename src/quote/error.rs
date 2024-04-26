use std::error;
use std::fmt;

#[derive(Debug)]
pub enum QuoteRequestError {
    HttpRequest(reqwest::Error),
    JsonParse(serde_json::Error),
    ParseBigDecimal(bigdecimal::ParseBigDecimalError),
    Other(String),
}

impl fmt::Display for QuoteRequestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            QuoteRequestError::HttpRequest(ref err) => write!(f, "HTTP Request Error: {}", err),
            QuoteRequestError::JsonParse(ref err) => write!(f, "JSON Parse Error: {}", err),
            QuoteRequestError::ParseBigDecimal(ref err) => {
                write!(f, "BigDecimal Parse Error: {}", err)
            }
            QuoteRequestError::Other(ref err) => write!(f, "Other Error: {}", err),
        }
    }
}

impl error::Error for QuoteRequestError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match *self {
            QuoteRequestError::HttpRequest(ref err) => Some(err),
            QuoteRequestError::JsonParse(ref err) => Some(err),
            QuoteRequestError::ParseBigDecimal(ref err) => Some(err),
            QuoteRequestError::Other(_) => None,
        }
    }
}

impl From<reqwest::Error> for QuoteRequestError {
    fn from(err: reqwest::Error) -> QuoteRequestError {
        QuoteRequestError::HttpRequest(err)
    }
}

impl From<serde_json::Error> for QuoteRequestError {
    fn from(err: serde_json::Error) -> QuoteRequestError {
        QuoteRequestError::JsonParse(err)
    }
}

impl From<bigdecimal::ParseBigDecimalError> for QuoteRequestError {
    fn from(err: bigdecimal::ParseBigDecimalError) -> QuoteRequestError {
        QuoteRequestError::ParseBigDecimal(err)
    }
}

impl From<&str> for QuoteRequestError {
    fn from(err: &str) -> QuoteRequestError {
        QuoteRequestError::Other(err.to_string())
    }
}

impl From<String> for QuoteRequestError {
    fn from(err: String) -> QuoteRequestError {
        QuoteRequestError::Other(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_display_other_error() {
        let err = QuoteRequestError::Other("test".to_string());
        assert_eq!("Other Error: test", format!("{}", err));
    }
}
