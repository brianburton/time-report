use anyhow::{Context, Result, anyhow};
use rand::Rng;
use regex::Captures;
use std::fmt::Display;
use std::fs::{OpenOptions, exists};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::str::FromStr;
use std::{fs, iter};

fn get_group_str<'a>(
    context: &str,
    text: &str,
    re_captures: &Option<Captures<'a>>,
    group: usize,
) -> Result<&'a str> {
    re_captures
        .as_ref()
        .and_then(|m| m.get(group))
        .map(|m| m.as_str())
        .ok_or_else(|| anyhow!(format!("{context}: cannot find value in {text}")))
}

pub fn parse_capture_group<T>(
    context: &str,
    text: &str,
    re_caps: &Option<Captures>,
    group: usize,
) -> Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    let digit_str = get_group_str(context, text, re_caps, group)?;
    let number = digit_str.parse::<T>().map_err(|e| {
        anyhow!(
            "{}: error parsing '{}' in '{}': {}",
            context,
            digit_str,
            text,
            e
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
    let error_context = "create_temp_file";
    let (mode, dir, name) = get_temp_file_specs(path)?;
    for _index in 0..50 {
        let temp_path = format!("{}_time_report_{}_{}", dir, random_chars(), name);
        if exists(&temp_path).with_context(|| format!("{}: exists", error_context))? {
            continue;
        }
        let _file = OpenOptions::new()
            .write(true)
            .create(true)
            .mode(mode)
            .open(&temp_path)
            .with_context(|| format!("{}: open", error_context))?;
        return Ok(temp_path);
    }
    Err(anyhow!("{}: unable to create temp file", error_context))
}

pub fn delete_file(temp_file: &str) -> Result<()> {
    let error_context = "delete_file";
    if exists(temp_file).with_context(|| format!("{}: exists", error_context))? {
        fs::remove_file(temp_file).with_context(|| format!("{}: remove_file", error_context))?;
    }
    Ok(())
}
