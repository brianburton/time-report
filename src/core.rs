use derive_getters::Getters;
use rand::Rng;
use regex::Captures;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::fs::{OpenOptions, exists};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::str::FromStr;
use std::{fs, iter};

#[derive(Getters)]
pub struct AppError {
    context: String,
    detail: String,
    source: Option<Box<dyn Error + 'static>>,
}

impl PartialEq<AppError> for AppError {
    fn eq(&self, other: &AppError) -> bool {
        self.to_string() == other.to_string()
    }
}

impl Debug for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cause = match self.source {
            Some(ref e) => format!(" cause=[{}]", e),
            None => String::new(),
        };
        write!(
            f,
            "context='{}' detail='{}'{}",
            self.context, self.detail, cause
        )
    }
}

impl Error for AppError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as &_)
    }
}

impl AppError {
    pub fn from_str(context: &str, detail: &str) -> Self {
        Self {
            context: context.to_string(),
            detail: detail.to_string(),
            source: None,
        }
    }

    pub fn from_error<E: Error + 'static>(context: &str, detail: &str, e: E) -> Self {
        Self {
            context: context.to_string(),
            detail: detail.to_string(),
            source: Some(Box::new(e)),
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
    let io_err =
        |detail: &str, e: std::io::Error| AppError::from_error("get_temp_file_specs", detail, e);
    let (dir, name) = split_path(path);
    let orig = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|e| io_err("open", e))?;
    let mode = orig
        .metadata()
        .map_err(|e| io_err("metadata", e))?
        .permissions()
        .mode();
    Ok((mode, dir, name))
}

pub fn create_temp_file(path: &str) -> Result<String, AppError> {
    let io_err =
        |detail: &str, e: std::io::Error| AppError::from_error("create_temp_file", detail, e);
    let (mode, dir, name) = get_temp_file_specs(path)?;
    for _index in 0..50 {
        let temp_path = format!("{}_time_report_{}_{}", dir, random_chars(), name);
        if exists(&temp_path).map_err(|e| io_err("exists", e))? {
            continue;
        }
        let _file = OpenOptions::new()
            .write(true)
            .create(true)
            .mode(mode)
            .open(&temp_path)
            .map_err(|e| io_err("open", e))?;
        return Ok(temp_path);
    }
    Err(AppError::from_str(
        "create_temp_file",
        "Unable to create temp file",
    ))
}

pub fn delete_file(temp_file: &str) -> Result<(), AppError> {
    let io_err = |detail: &str, e: std::io::Error| AppError::from_error("delete_file", detail, e);
    if exists(temp_file).map_err(|e| io_err("exists", e))? {
        fs::remove_file(temp_file).map_err(|e| io_err("remove_file", e))?;
    }
    Ok(())
}
