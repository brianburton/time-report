use crate::core::AppError;
use crate::model::{Date, DayEntry, ProjectTimes, Time, TimeRange};
use im::Vector;
use lazy_static::lazy_static;
use regex::Regex;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

lazy_static! {
    static ref COMMENT_RE: Regex = Regex::new(r"^(.*)\s*--.*$").unwrap();
    static ref TIME_RANGE_RE: Regex = Regex::new(r"(\d{4})-(\d{4})").unwrap();
    static ref TIME_RANGES_RE: Regex =
        Regex::new(r"^(\d{4}-\d{4}(,\d{4}-\d{4})*)(,\d{4}-)?$").unwrap();
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

fn parse_time(hhmm: &str) -> Result<Time, AppError> {
    Time::parse(hhmm)
}

fn parse_time_range(text: &str) -> Result<TimeRange, AppError> {
    let caps = TIME_RANGE_RE
        .captures(text)
        .ok_or_else(|| AppError::from_str("time range", "not a time range"))?;
    let from = parse_time(caps[1].to_string().as_str())?;
    let to = parse_time(caps[2].to_string().as_str())?;
    TimeRange::new(from, to)
}

// Function to parse the time ranges from a string (e.g., "0800-1200,1300-1310,1318-1708")
fn parse_time_ranges(time_range_str: &str) -> Result<(Vector<TimeRange>, bool), AppError> {
    let caps = TIME_RANGES_RE.captures(time_range_str).ok_or_else(|| {
        AppError::from_str(
            "time ranges",
            &format!("invalid time ranges: {}", time_range_str),
        )
    })?;

    let mut time_ranges = Vector::new();

    for cap in TIME_RANGE_RE.captures_iter(caps[1].to_string().as_str()) {
        let text = cap[0].to_string();
        let tr = parse_time_range(text.as_str())?;
        time_ranges.push_back(tr);
    }

    Ok((time_ranges, caps.get(3).is_some()))
}

fn is_date_line(line: &str) -> bool {
    DATE_LINE_RE.find(line).is_some()
}

fn is_time_line(line: &str) -> bool {
    TIME_LINE_RE.find(line).is_some()
}

// Function to parse a date line (e.g., "Date: Thursday 04/03/2025")
fn parse_date_line(line: &str) -> Result<Date, AppError> {
    let caps = DATE_LINE_RE
        .captures(line)
        .ok_or_else(|| AppError::from_str("date line", "not a date line"))?;
    Date::parse(caps[1].to_string().as_str())
}

// Function to parse label and time ranges (e.g., "client,project: 0800-1200,1300-1310")
fn parse_time_line(line: &str) -> Result<(ProjectTimes, bool), AppError> {
    let caps = TIME_LINE_RE
        .captures(line)
        .ok_or_else(|| AppError::from_str("time line", "not a time line"))?;
    let client = caps[1].to_string();
    let project = caps[2].to_string();
    let (time_ranges, incomplete) = parse_time_ranges(&caps[3])?;
    Ok((
        ProjectTimes::new(client.as_str(), project.as_str(), &time_ranges)?,
        incomplete,
    ))
}

// Function to parse a file into day entries
pub fn parse_file(file_path: &str) -> Result<(Vector<DayEntry>, Vector<String>), AppError> {
    let path = Path::new(file_path);
    let file = File::open(path).map_err(|e| AppError::from_error("i/o", e))?;
    let reader = io::BufReader::new(file);

    let mut days = Vector::new();
    let mut have_date = false;
    let mut date = Date::min_date();
    let mut projects = Vector::new();
    let mut warnings = Vector::new();
    let mut line_num = 0;

    for raw_line in reader.lines() {
        line_num += 1;
        let line = raw_line
            .map(|s| remove_comments(&s))
            .map_err(|e| AppError::from_error("i/o", e))?;

        if is_date_line(line.as_str()) {
            if have_date {
                days.push_back(DayEntry::new(date, &projects));
            } else {
                have_date = true;
            }
            date = parse_date_line(&line)?;
            projects.clear();
        } else if is_time_line(line.as_str()) {
            if have_date {
                let (time_ranges, incomplete) = parse_time_line(&line)?;
                if incomplete {
                    warnings.push_back(format!(
                        "incomplete time range:{line_num}: date: {} line: {}",
                        &date,
                        line.as_str()
                    ));
                }
                projects.push_back(time_ranges);
            } else {
                return Err(AppError::from_str(
                    "file",
                    format!("time line before any dates:{line_num}: {}", line).as_str(),
                ));
            }
        } else if line == "END" {
            break;
        } else if !line.is_empty() {
            warnings.push_back(format!("invalid line:{line_num}: line: {}", line.as_str()));
        }
    }

    if have_date {
        days.push_back(DayEntry::new(date, &projects));
    }

    Ok((days, warnings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use im::vector;

    fn time(h: u16, m: u16) -> Time {
        Time::new(h, m).unwrap()
    }

    fn time_range(h1: u16, m1: u16, h2: u16, m2: u16) -> TimeRange {
        TimeRange::new(time(h1, m1), time(h2, m2)).unwrap()
    }

    #[test]
    fn test_parse_time_ranges() {
        let time_range_str = "0800-1200,1300-1310,1318-1708";
        let expected = vector![
            time_range(8, 0, 12, 0),
            time_range(13, 0, 13, 10),
            time_range(13, 18, 17, 8),
        ];

        assert_eq!(parse_time_ranges(time_range_str).unwrap().0, expected);
    }

    #[test]
    fn test_remove_comments() {
        assert_eq!("", remove_comments(""));
        assert_eq!("", remove_comments("--"));
        assert_eq!("", remove_comments("    --  ignored"));
        assert_eq!("xyz", remove_comments(" xyz   --  ignored"));
        assert_eq!("xyz", remove_comments(" xyz --first  --  second"));
    }

    #[test]
    fn test_parse_date_line() {
        let line = "Date: Thursday 04/03/2025";
        let expected = Date::new(2025, 4, 3).unwrap();

        assert_eq!(parse_date_line(line).unwrap(), expected);
    }

    #[test]
    fn test_parse_label_line() {
        let line = "abc,xyz: 0800-1200,1300-1310,1318-1708";
        let expected = ProjectTimes::new(
            "abc",
            "xyz",
            &vector![
                time_range(8, 0, 12, 0),
                time_range(13, 0, 13, 10),
                time_range(13, 18, 17, 8),
            ],
        );
        assert_eq!(parse_time_line(line).unwrap().0, expected.unwrap());
    }

    #[test]
    fn test_parse_file() {
        let file_content = "Date: Thursday 04/03/2025\n\nabc,xyz: 0800-1200,1300-1310,1318-1708\ndef,uvw: 1200-1300\n";
        let file_path = "test_file.txt";
        std::fs::write(file_path, file_content).unwrap();

        let expected = vector!(DayEntry::new(
            Date::new(2025, 4, 3).unwrap(),
            &vector!(
                ProjectTimes::new(
                    "abc",
                    "xyz",
                    &vector!(
                        time_range(8, 0, 12, 0),
                        time_range(13, 0, 13, 10),
                        time_range(13, 18, 17, 8),
                    ),
                )
                .unwrap(),
                ProjectTimes::new("def", "uvw", &vector!(time_range(12, 0, 13, 0),),).unwrap(),
            ),
        ));

        let result = parse_file(file_path).unwrap();

        assert_eq!(result.0, expected);

        std::fs::remove_file(file_path).unwrap(); // Clean up test file
    }
}
