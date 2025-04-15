use derive_getters::Getters;
use regex::Captures;
use std::error::Error;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Debug, Getters, PartialEq)]
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

fn get_group_str<'a>(
    context: &str,
    text: &str,
    re_captures: &Option<Captures<'a>>,
    group: usize,
) -> Result<&'a str, AppError> {
    re_captures
        .as_ref()
        .and_then(|m| m.get(group))
        .map(|m| m.as_str())
        .ok_or_else(|| AppError::from_str(context, format!("cannot find value in {text}").as_str()))
}

pub fn parse_capture_group<T>(
    context: &str,
    text: &str,
    re_caps: &Option<Captures>,
    group: usize,
) -> Result<T, AppError>
where
    T: FromStr,
    T::Err: Display,
{
    let digit_str = get_group_str(context, text, re_caps, group)?;
    let number = digit_str.parse::<T>().map_err(|e| {
        AppError::from_str(
            context,
            format!(
                "error parsing '{}' in '{}': {}",
                digit_str,
                text,
                e.to_string()
            )
            .as_str(),
        )
    })?;
    Ok(number)
}
