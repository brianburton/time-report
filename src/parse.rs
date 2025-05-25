use crate::model::{Date, DayEntry, Project, ProjectTimes, Time, TimeRange};
use anyhow::Result;
use anyhow::{Context, anyhow};
use im::Vector;
use lazy_static::lazy_static;
use regex::Regex;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

#[cfg(test)]
mod tests;

lazy_static! {
    static ref COMMENT_RE: Regex = Regex::new(r"^(.*)\s*--.*$").unwrap();
    static ref PARTIAL_TIME_RANGE_RE: Regex = Regex::new(r"\d{4}-$").unwrap();
    static ref TIME_RANGE_RE: Regex = Regex::new(r"(\d{4})-(\d{4})").unwrap();
    static ref TIME_RANGES_RE: Regex = Regex::new(
        r"(^\d{4}-\d{4}(,\d{4}-\d{4})*(,\d{4}-)?$)|(^\d{4}-\d{4}(,\d{4}-\d{4})*$)|(^\d{4}-$)"
    )
    .unwrap();
    static ref EMPTY_TIME_LINE_RE: Regex =
        Regex::new(r"^([a-z]+),([-/ A-Za-z0-9]+) *: *$").unwrap();
    static ref TIME_LINE_RE: Regex = Regex::new(r"^([a-z]+),([-/ A-Za-z0-9]+) *: *(.*)$").unwrap();
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
        .ok_or_else(|| anyhow!("parse_time_range: not a time range"))?;
    let from = parse_time(caps[1].to_string().as_str())?;
    let to = parse_time(caps[2].to_string().as_str())?;
    TimeRange::new(from, to)
}

// Function to parse the time ranges from a string (e.g., "0800-1200,1300-1310,1318-1708")
fn parse_time_ranges(time_range_str: &str) -> Result<(Vector<TimeRange>, bool)> {
    if !TIME_RANGES_RE.is_match(time_range_str) {
        return Err(anyhow!(
            "parse_time_ranges: invalid time ranges: {}",
            time_range_str
        ));
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
        .ok_or_else(|| anyhow!("parse_date_line: not a date line"))?;
    Date::parse(caps[1].to_string().as_str())
}

// Function to parse label and time ranges (e.g., "client,project: 0800-1200,1300-1310")
fn parse_time_line(line: &str) -> Result<(ProjectTimes, bool)> {
    let caps = TIME_LINE_RE
        .captures(line)
        .ok_or_else(|| anyhow!("parse_time_line: not a time line"))?;
    let client = caps[1].to_string();
    let project = caps[2].to_string();
    let (time_ranges, incomplete) = parse_time_ranges(&caps[3])?;
    Ok((
        ProjectTimes::new(
            Project::new(client.as_str(), project.as_str()),
            &time_ranges,
        )?,
        incomplete,
    ))
}

// Function to parse a file into day entries
pub fn parse_file(file_path: &str) -> Result<(Vector<DayEntry>, Vector<String>)> {
    let path = Path::new(file_path);
    let file = File::open(path).with_context(|| "parse_file: open")?;
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
            .with_context(|| "parse_file: read")?;

        if is_date_line(line.as_str()) {
            date_line_num = line_num;
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
                return Err(anyhow!(
                    "parse_file: time line before any dates:{line_num}: '{}'",
                    line
                ));
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
