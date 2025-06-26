use anyhow::{Context, Error, Result, bail};
use rand::Rng;
use regex::Captures;
use std::fs::{OpenOptions, exists};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::str::FromStr;
use std::{fs, iter};
use thiserror::Error;

#[derive(Error, Debug)]
enum CoreError {
    #[error("Capture group {1} missing in text: {0}")]
    MissingCaptureGroup(String, usize),
    #[error("Error parsing capture group for {0}: {1}")]
    CaptureGroupParsingError(String, #[source] anyhow::Error),
    #[error("Error creating temp file: {0}")]
    TempFileCreationError(#[source] std::io::Error),
    #[error("Unable to assign unique name for temp file")]
    TempFileNamingError,
    #[error("Unable to delete file {0}: {1}")]
    DeleteFileError(String, #[source] std::io::Error),
}

fn get_group_str<'a>(re_captures: &Option<Captures<'a>>, group: usize) -> Option<&'a str> {
    re_captures
        .as_ref()
        .and_then(|m| m.get(group))
        .map(|m| m.as_str())
}

pub fn parse_capture_group<T>(context: &str, re_caps: &Option<Captures>, group: usize) -> Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let digit_str = get_group_str(re_caps, group)
        .ok_or_else(|| CoreError::MissingCaptureGroup(context.to_string(), group))?;
    let number = digit_str
        .parse::<T>()
        .map_err(|e| CoreError::CaptureGroupParsingError(context.to_string(), Error::from(e)))?;
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

fn get_temp_file_specs(path: &str) -> Result<(u32, &str, &str)> {
    let error_context = "get_temp_file_specs";
    let (dir, name) = split_path(path);
    let orig = OpenOptions::new()
        .read(true)
        .open(path)
        .with_context(|| format!("{}: open", error_context))?;
    let mode = orig
        .metadata()
        .with_context(|| format!("{}: metadata", error_context))?
        .permissions()
        .mode();
    Ok((mode, dir, name))
}

pub fn create_temp_file(path: &str) -> Result<String> {
    let (mode, dir, name) = get_temp_file_specs(path)?;
    for _index in 0..50 {
        let temp_path = format!("{}_time_report_{}_{}", dir, random_chars(), name);
        if exists(&temp_path).map_err(CoreError::TempFileCreationError)? {
            continue;
        }
        let _file = OpenOptions::new()
            .write(true)
            .create(true)
            .mode(mode)
            .open(&temp_path)
            .map_err(CoreError::TempFileCreationError)?;
        return Ok(temp_path);
    }
    bail!(CoreError::TempFileNamingError);
}

pub fn delete_file(temp_file: &str) -> Result<()> {
    if exists(temp_file).map_err(|e| CoreError::DeleteFileError(temp_file.to_string(), e))? {
        fs::remove_file(temp_file)
            .map_err(|e| CoreError::DeleteFileError(temp_file.to_string(), e))?;
    }
    Ok(())
}
