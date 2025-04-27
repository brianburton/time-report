use derive_getters::Getters;
use rand::Rng;
use regex::Captures;
use std::error::Error;
use std::fmt::Display;
use std::fs::{OpenOptions, exists};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::str::FromStr;
use std::{fs, iter};

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

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::from_error("IO Error", &error)
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
            format!("error parsing '{}' in '{}': {}", digit_str, text, e).as_str(),
        )
    })?;
    Ok(number)
}

fn random_chars() -> String {
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
    let mut rng = rand::rng();
    let one_char = || CHARSET[rng.random_range(0..CHARSET.len())] as char;
    iter::repeat_with(one_char).take(7).collect()
}

fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[..=i], &path[i + 1..]),
        None => ("", path),
    }
}

fn get_temp_file_specs(path: &str) -> Result<(u32, &str, &str), AppError> {
    let (dir, name) = split_path(path);
    let orig = OpenOptions::new().read(true).open(path)?;
    let mode = orig.metadata()?.permissions().mode();
    Ok((mode, dir, name))
}

pub fn create_temp_file(path: &str) -> Result<String, AppError> {
    let (mode, dir, name) = get_temp_file_specs(path)?;
    for _index in 0..50 {
        let temp_path = format!("{}_time_report_{}_{}", dir, random_chars(), name);
        if exists(&temp_path)? {
            continue;
        }
        let _file = OpenOptions::new()
            .write(true)
            .create(true)
            .mode(mode)
            .open(&temp_path)?;
        return Ok(temp_path);
    }
    Err(AppError::from_str("output", "Failed to create temp file"))
}

pub fn delete_file(temp_file: &str) -> Result<(), AppError> {
    if exists(temp_file)? {
        fs::remove_file(temp_file)?;
    }
    Ok(())
}
