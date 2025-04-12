use derive_getters::Getters;
use regex::Regex;
use std::error::Error;
use std::fmt::Display;

#[derive(Debug, Getters,PartialEq)]
pub struct AppError {
    context: String,
    detail: String,
}

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "error: context: {} detail: {}",
            self.context, self.detail
        )
    }
}

impl Error for AppError {}

impl AppError {
    pub fn from_str(context: &str, detail: &str) -> Self {
        Self {
            context: context.to_string(),
            detail: detail.to_string(),
        }
    }

    pub fn from_error<E: Error>(context: &str, e: E) -> Self {
        Self {
            context: context.to_string(),
            detail: e.to_string(),
        }
    }
}

pub fn parse_digits(context: &str, re: &Regex, text: &str, group: usize) -> Result<u16, AppError> {
    let digit_str = re
        .captures(text)
        .and_then(|m| m.get(group))
        .map(|m| m.as_str())
        .ok_or_else(|| {
            AppError::from_str(context, format!("cannot find value in {text}").as_str())
        })?;
    let number = digit_str
        .parse::<u16>()
        .map_err(|e| AppError::from_error(context, e))?;
    Ok(number)
}
