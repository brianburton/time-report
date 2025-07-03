use crate::model::{Date, DayEntry, Project, ProjectTimes, Time, TimeRange};
use anyhow::{Result, bail};
use im::Vector;
use lazy_static::lazy_static;
use regex::Regex;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;
use thiserror::Error;

#[cfg(test)]
mod tests;

#[derive(Error, Debug)]
enum ParseError {
    #[error("Invalid time range: {0}")]
    InvalidTimeRange(String),
    #[error("Invalid time ranges: {0}")]
    InvalidTimeRanges(String),
    #[error("Invalid time line: {0}")]
    InvalidTimeLine(String),
    #[error("Invalid date line: {0}")]
    InvalidDateLine(String),
    #[error("Time line appears before first date: line {0}: {1}")]
    TimeLineWithNoDate(u32, String),
    #[error("Unable to open file {0}: {1}")]
    OpenFileFailure(String, #[source] io::Error),
    #[error("Unable to read line from file: {0}")]
    ReadFileFailure(#[from] io::Error),
}

lazy_static! {
    static ref COMMENT_RE: Regex = Regex::new(r"^(.*)\s*--.*$").unwrap();
    static ref PARTIAL_TIME_RANGE_RE: Regex = Regex::new(r"\d{4}-$").unwrap();
    static ref TIME_RANGE_RE: Regex = Regex::new(r"(\d{4})-(\d{4})").unwrap();
    static ref TIME_RANGES_RE: Regex = Regex::new(
        r"(^\d{4}-\d{4}(,\d{4}-\d{4})*(,\d{4}-)?$)|(^\d{4}-\d{4}(,\d{4}-\d{4})*$)|(^\d{4}-$)"
    )
    .unwrap();
    static ref EMPTY_TIME_LINE_RE: Regex = Regex::new(
        r"^(?<client>[a-z]+),(?<code>[-/ A-Za-z0-9]+)(,(?<subcode>[-/ A-Za-z0-9]+))? *: *$"
    )
    .unwrap();
    static ref TIME_LINE_RE: Regex = Regex::new(
        r"^(?<client>[a-z]+),(?<code>[-/ A-Za-z0-9]+)(,(?<subcode>[-/ A-Za-z0-9]+))? *: *(?<times>.*)$"
    )
    .unwrap();
    static ref DATE_LINE_RE: Regex = Regex::new(r"^Date: [A-Za-z]+ (\d{2}/\d{2}/\d{4})$").unwrap();
}

fn remove_comments(source: &str) -> String {
    let mut current: String = source.to_string();
    loop {
        if let Some(caps) = COMMENT_RE.captures(current.as_str()) {
            let changed = &caps[1];
            if current != changed {
                current = changed.trim().to_string();
                continue;
            }
        };
        break;
    }
    current
}

fn parse_time(hhmm: &str) -> Result<Time> {
    Time::parse(hhmm)
}

fn parse_time_range(text: &str) -> Result<TimeRange> {
    let caps = TIME_RANGE_RE
        .captures(text)
        .ok_or_else(|| ParseError::InvalidTimeRange(text.to_string()))?;
    let from = parse_time(caps[1].to_string().as_str())?;
    let to = parse_time(caps[2].to_string().as_str())?;
    TimeRange::new(from, to)
}

// Function to parse the time ranges from a string (e.g., "0800-1200,1300-1310,1318-1708")
fn parse_time_ranges(time_range_str: &str) -> Result<(Vector<TimeRange>, bool)> {
    if !TIME_RANGES_RE.is_match(time_range_str) {
        bail!(ParseError::InvalidTimeRanges(time_range_str.to_string()));
    };

    let mut time_ranges = Vector::new();

    for cap in TIME_RANGE_RE.captures_iter(time_range_str) {
        let text = cap[0].to_string();
        let tr = parse_time_range(text.as_str())?;
        time_ranges.push_back(tr);
    }

    Ok((time_ranges, PARTIAL_TIME_RANGE_RE.is_match(time_range_str)))
}

fn is_date_line(line: &str) -> bool {
    DATE_LINE_RE.find(line).is_some()
}

fn is_time_line(line: &str) -> bool {
    TIME_LINE_RE.find(line).is_some()
}

fn is_empty_time_line(line: &str) -> bool {
    EMPTY_TIME_LINE_RE.find(line).is_some()
}

// Function to parse a date line (e.g., "Date: Thursday 04/03/2025")
fn parse_date_line(line: &str) -> Result<Date> {
    let caps = DATE_LINE_RE
        .captures(line)
        .ok_or_else(|| ParseError::InvalidDateLine(line.to_string()))?;
    Date::parse(caps[1].to_string().as_str())
}

pub fn try_parse_date_line(line: &str) -> Option<Date> {
    parse_date_line(line).ok()
}

// Function to parse label and time ranges (e.g., "client,code,subcode: 0800-1200,1300-1310")
fn parse_time_line(line: &str) -> Result<(ProjectTimes, bool)> {
    let caps = TIME_LINE_RE
        .captures(line)
        .ok_or_else(|| ParseError::InvalidTimeLine(line.to_string()))?;
    let client = caps["client"].to_string();
    let code = caps["code"].to_string();
    let subcode = caps.name("subcode").map_or("", |m| m.as_str()).to_string();
    let (time_ranges, incomplete) = parse_time_ranges(&caps["times"])?;
    Ok((
        ProjectTimes::new(
            Project::new(client.as_str(), code.as_str(), subcode.as_str()),
            &time_ranges,
        )?,
        incomplete,
    ))
}

// Function to parse a file into day entries
pub fn parse_file(file_path: &str) -> Result<(Vector<DayEntry>, Vector<String>)> {
    let path = Path::new(file_path);
    let file =
        File::open(path).map_err(|e| ParseError::OpenFileFailure(file_path.to_string(), e))?;
    let reader = io::BufReader::new(file);

    let mut days = Vector::new();
    let mut have_date = false;
    let mut date = Date::min_date();
    let mut projects = Vector::new();
    let mut warnings = Vector::new();
    let mut line_num = 0;

    let mut date_line_num = 0;
    for raw_line in reader.lines() {
        line_num += 1;
        let line = raw_line
            .map(|s| remove_comments(&s))
            .map_err(ParseError::ReadFileFailure)?;

        if is_date_line(line.as_str()) {
            let new_date = parse_date_line(&line)?;
            if have_date {
                if new_date <= date {
                    warnings.push_back(format!(
                        "out of order dates:{line_num}: prev='{date}' new='{new_date}'"
                    ));
                }
                days.push_back(DayEntry::new(date, &projects, date_line_num));
            } else {
                have_date = true;
            }
            date = new_date;
            date_line_num = line_num;
            projects.clear();
        } else if is_empty_time_line(line.as_str()) {
            warnings.push_back(format!(
                "incomplete time line:{line_num}: line: '{}'",
                line.as_str()
            ));
        } else if is_time_line(line.as_str()) {
            if have_date {
                let (time_ranges, incomplete) = parse_time_line(&line)?;
                if incomplete {
                    warnings.push_back(format!(
                        "incomplete time range:{line_num}: date='{}' line='{}'",
                        &date,
                        line.as_str()
                    ));
                }
                projects.push_back(time_ranges);
            } else {
                bail!(ParseError::TimeLineWithNoDate(line_num, line));
            }
        } else if line == "END" {
            break;
        } else if !line.is_empty() {
            warnings.push_back(format!(
                "invalid line:{line_num}: line: '{}'",
                line.as_str()
            ));
        }
    }

    if have_date {
        days.push_back(DayEntry::new(date, &projects, date_line_num));
    }

    Ok((days, warnings))
}
