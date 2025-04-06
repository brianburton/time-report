use core::AppError;
use im::Vector;
use lazy_static::lazy_static;
use model::{Date, DayEntry, ProjectTimes, Time, TimeRange};
use regex::Regex;
use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

mod core {
    use derive_getters::Getters;
    use regex::Regex;
    use std::error::Error;
    use std::fmt::Display;

    #[derive(Debug, Getters)]
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

    pub fn parse_digits(
        context: &str,
        re: &Regex,
        text: &str,
        group: usize,
    ) -> Result<u16, AppError> {
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
}

mod model {
    use crate::parse::core::{AppError, parse_digits};
    use derive_getters::Getters;
    use im::{HashSet, Vector, hashset};
    use lazy_static::lazy_static;
    use regex::Regex;

    lazy_static! {
        static ref TIME_RE: Regex = Regex::new(r"(\d{2})(\d{2})").unwrap();
        static ref DATE_RE: Regex = Regex::new(r"(\d{2})/(\d{2})/(\d{4})").unwrap();
        static ref LONG_MONTHS: HashSet<u16> = hashset!(1, 3, 5, 7, 8, 10, 12);
        static ref SHORT_MONTHS: HashSet<u16> = hashset!(4, 6, 9, 11);
    }

    pub const MIN_YEAR: u16 = 1970;
    pub const MAX_YEAR: u16 = 2300;

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
    pub struct Time {
        minute: u16,
    }

    impl Time {
        pub fn new(hour: u16, minute: u16) -> Result<Time, AppError> {
            if !is_valid_time(hour, minute) {
                Err(AppError::from_str("time", "not a valid time"))
            } else {
                let minute = hour * 60 + minute;
                Ok(Time { minute })
            }
        }

        pub fn parse(text: &str) -> Result<Time, AppError> {
            let h = parse_digits("hour", &TIME_RE, text, 1)?;
            let m = parse_digits("minute", &TIME_RE, text, 2)?;
            Self::new(h, m)
        }

        pub fn hour(&self) -> u16 {
            self.minute / 60
        }

        pub fn minute(&self) -> u16 {
            self.minute % 60
        }
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Getters)]
    pub struct Date {
        year: u16,
        month: u16,
        day: u16,
    }

    impl Date {
        pub fn new(year: u16, month: u16, day: u16) -> Result<Date, AppError> {
            if !is_valid_date(year, month, day) {
                Err(AppError::from_str("date", "not a valid date"))
            } else {
                Ok(Date { year, month, day })
            }
        }

        pub fn parse(text: &str) -> Result<Date, AppError> {
            let m = parse_digits("month", &DATE_RE, text, 1)?;
            let d = parse_digits("day", &DATE_RE, text, 2)?;
            let y: u16 = parse_digits("year", &DATE_RE, text, 3)?;
            Self::new(y, m, d)
        }
    }

    #[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Getters)]
    pub struct TimeRange {
        from: Time,
        to: Time,
    }

    impl TimeRange {
        pub fn new(from: Time, to: Time) -> Result<TimeRange, AppError> {
            if from >= to {
                Err(AppError::from_str("model", "out of order time range"))
            } else {
                Ok(TimeRange { from, to })
            }
        }
    }

    #[derive(Debug, PartialEq, Clone, Getters)]
    pub struct ProjectTimes {
        client: String,
        project: String,
        time_ranges: Vector<TimeRange>,
    }

    impl ProjectTimes {
        pub fn new(client: &str, project: &str, time_ranges: &Vector<TimeRange>) -> ProjectTimes {
            let mut sorted = time_ranges.clone();
            sorted.sort();
            ProjectTimes {
                client: client.to_string(),
                project: project.to_string(),
                time_ranges: sorted,
            }
        }
    }

    #[derive(Debug, PartialEq, Clone, Getters)]
    pub struct DayEntry {
        date: Date,
        projects: Vector<ProjectTimes>,
    }

    impl DayEntry {
        pub fn new(date: Date) -> Self {
            DayEntry {
                date,
                projects: Vector::new(),
            }
        }

        pub fn new2(date: Date, projects: &Vector<ProjectTimes>) -> Self {
            DayEntry {
                date,
                projects: projects.clone(),
            }
        }

        pub fn add_project(&mut self, project_times: ProjectTimes) {
            self.projects.push_back(project_times);
        }
    }

    fn is_valid_time(hour: u16, minute: u16) -> bool {
        hour < 24 && minute < 60
    }

    fn is_leap_year(year: u16) -> bool {
        (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
    }

    fn days_in_month(year: u16, month: u16) -> u16 {
        if LONG_MONTHS.contains(&month) {
            31
        } else if SHORT_MONTHS.contains(&month) {
            30
        } else if is_leap_year(year) {
            29
        } else {
            28
        }
    }

    fn days_in_year(year: u16) -> u16 {
        if is_leap_year(year) { 366 } else { 365 }
    }

    fn day_of_year(year: u16, month: u16, day: u16) -> u16 {
        if month == 1 {
            day
        } else {
            day + (1..month).fold(0, |s, m| s + days_in_month(year, m))
        }
    }

    fn day_number(year: u16, month: u16, day: u16) -> u16 {
        let past_year_days = (MIN_YEAR..year).fold(0, |s, y| s + days_in_year(y));
        let past_month_days = (1..month).fold(0, |s, m| s + days_in_month(year, m));
        past_year_days + past_month_days + day
    }

    fn is_valid_date(year: u16, month: u16, day: u16) -> bool {
        (MIN_YEAR..=MAX_YEAR).contains(&year)
            && (1..=12).contains(&month)
            && day >= 1
            && day <= days_in_month(year, month)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn time(h: u16, m: u16) -> Time {
            Time::new(h, m).unwrap()
        }

        #[test]
        fn test_time() {
            assert_eq!(0, time(0, 0).hour());
            assert_eq!(0, time(0, 59).hour());
            assert_eq!(1, time(1, 0).hour());
            assert_eq!(1, time(1, 59).hour());
            assert_eq!(23, time(23, 0).hour());
            assert_eq!(23, time(23, 59).hour());

            assert_eq!(0, time(0, 0).minute());
            assert_eq!(59, time(0, 59).minute());
            assert_eq!(0, time(1, 0).minute());
            assert_eq!(59, time(1, 59).minute());
            assert_eq!(0, time(23, 0).minute());
            assert_eq!(59, time(23, 59).minute());
        }

        #[test]
        fn test_date_time_valid() {
            assert_eq!(true, is_valid_time(0, 0));
            assert_eq!(true, is_valid_time(23, 59));
            assert_eq!(false, is_valid_time(24, 0));
            assert_eq!(false, is_valid_time(0, 60));
            assert_eq!(true, is_valid_date(MIN_YEAR, 1, 1));
            assert_eq!(true, is_valid_date(MAX_YEAR, 12, 31));
            assert_eq!(false, is_valid_date(MIN_YEAR - 1, 12, 31));
            assert_eq!(false, is_valid_date(MAX_YEAR + 1, 1, 1));
            assert_eq!(false, is_valid_date(MAX_YEAR, 13, 1));
            assert_eq!(false, is_valid_date(MAX_YEAR, 1, 32));
        }

        #[test]
        fn test_day_of_year_leap_year() {
            let cases = [
                (1, 1, 1),
                (1, 31, 31),
                (2, 1, 32),
                (2, 29, 60),
                (3, 1, 61),
                (3, 31, 91),
                (4, 1, 92),
                (4, 30, 121),
                (5, 1, 122),
                (5, 31, 152),
                (6, 1, 153),
                (6, 30, 182),
                (7, 1, 183),
                (7, 31, 213),
                (8, 1, 214),
                (8, 31, 244),
                (9, 1, 245),
                (9, 30, 274),
                (10, 1, 275),
                (10, 31, 305),
                (11, 1, 306),
                (11, 30, 335),
                (12, 1, 336),
                (12, 31, 366),
            ];

            cases
                .iter()
                .for_each(|(m, d, e)| assert_eq!(*e, day_of_year(2000, *m, *d)));

            assert_eq!(366, days_in_year(2000));
        }

        #[test]
        fn test_day_of_year_normal_year() {
            let cases = [
                (1, 1, 1),
                (1, 31, 31),
                (2, 1, 32),
                (2, 28, 59),
                (3, 1, 60),
                (3, 31, 90),
                (4, 1, 91),
                (4, 30, 120),
                (5, 1, 121),
                (5, 31, 151),
                (6, 1, 152),
                (6, 30, 181),
                (7, 1, 182),
                (7, 31, 212),
                (8, 1, 213),
                (8, 31, 243),
                (9, 1, 244),
                (9, 30, 273),
                (10, 1, 274),
                (10, 31, 304),
                (11, 1, 305),
                (11, 30, 334),
                (12, 1, 335),
                (12, 31, 365),
            ];

            cases
                .iter()
                .for_each(|(m, d, e)| assert_eq!(*e, day_of_year(2001, *m, *d)));

            assert_eq!(365, days_in_year(2001));
        }

        #[test]
        fn test_day_number() {
            assert_eq!(1, day_number(MIN_YEAR, 1, 1));
            assert_eq!(366, day_number(MIN_YEAR + 1, 1, 1));

            let base = day_number(2000, 12, 31);
            assert_eq!(11323, base);

            let cases = [
                (1, 1, base + 1),
                (1, 31, base + 31),
                (2, 1, base + 32),
                (2, 28, base + 59),
                (3, 1, base + 60),
                (3, 31, base + 90),
                (4, 1, base + 91),
                (4, 30, base + 120),
                (5, 1, base + 121),
                (5, 31, base + 151),
                (6, 1, base + 152),
                (6, 30, base + 181),
                (7, 1, base + 182),
                (7, 31, base + 212),
                (8, 1, base + 213),
                (8, 31, base + 243),
                (9, 1, base + 244),
                (9, 30, base + 273),
                (10, 1, base + 274),
                (10, 31, base + 304),
                (11, 1, base + 305),
                (11, 30, base + 334),
                (12, 1, base + 335),
                (12, 31, base + 365),
            ];

            cases
                .iter()
                .for_each(|(m, d, num)| assert_eq!(*num, day_number(2001, *m, *d)));
        }

        #[test]
        fn test_days_in_month() {
            [1, 3, 5, 7, 8, 10, 12]
                .iter()
                .for_each(|m| assert_eq!(31, days_in_month(2000, *m)));
            [4, 6, 9, 11]
                .iter()
                .for_each(|m| assert_eq!(30, days_in_month(2000, *m)));
            [2001, 2002, 2003, 2100, 2200, 2300]
                .iter()
                .for_each(|y| assert_eq!(28, days_in_month(*y, 2)));
            [1996, 2000, 2004]
                .iter()
                .for_each(|y| assert_eq!(29, days_in_month(*y, 2)));
        }
    }
}

lazy_static! {
    static ref COMMENT_RE: Regex = Regex::new(r"^(.*)\s*--.*$").unwrap();
    static ref TIME_RANGE_RE: Regex = Regex::new(r"(\d{4})-(\d{4})").unwrap();
    static ref TIME_LINE_RE: Regex = Regex::new(r"([a-z]+),([- A-Za-z]+) *: (.*)").unwrap();
    static ref DATE_LINE_RE: Regex = Regex::new(r"Date: [A-Za-z]+ (\d{2}/\d{2}/\d{4})").unwrap();
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
fn parse_time_ranges(time_range_str: &str) -> Result<Vector<TimeRange>, AppError> {
    let mut time_ranges = Vector::new();

    for cap in TIME_RANGE_RE.captures_iter(time_range_str) {
        let text = cap[0].to_string();
        let tr = parse_time_range(text.as_str())?;
        time_ranges.push_back(tr);
    }

    Ok(time_ranges)
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
fn parse_time_line(line: &str) -> Result<ProjectTimes, AppError> {
    let caps = TIME_LINE_RE
        .captures(line)
        .ok_or_else(|| AppError::from_str("time line", "not a time line"))?;
    let client = caps[1].to_string();
    let project = caps[2].to_string();
    let time_ranges = parse_time_ranges(&caps[3])?;
    Ok(ProjectTimes::new(
        client.as_str(),
        project.as_str(),
        &time_ranges,
    ))
}

// Function to parse a file into day entries
pub fn parse_file(file_path: &str) -> Result<Vector<DayEntry>, AppError> {
    let mut day_entries = Vector::new();
    let path = Path::new(file_path);
    let file = File::open(path).map_err(|e| AppError::from_error("i/o", e))?;
    let reader = io::BufReader::new(file);

    let mut current_day: Option<DayEntry> = None;

    for raw_line in reader.lines() {
        let line = raw_line
            .map(|s| remove_comments(&s))
            .map_err(|e| AppError::from_error("i/o", e))?;

        if is_date_line(line.as_str()) {
            if let Some(day_entry) = current_day.take() {
                day_entries.push_back(day_entry);
            }
            let date = parse_date_line(&line)?;
            current_day = Some(DayEntry::new(date));
        } else if is_time_line(line.as_str()) {
            let project_times = parse_time_line(&line)?;
            if let Some(day_entry) = &mut current_day {
                day_entry.add_project(project_times);
            } else {
                return Err(AppError::from_str(
                    "file",
                    format!("time line before any dates: {}", line).as_str(),
                ));
            }
        } else if line == "END" {
            break;
        } else if !line.is_empty() {
            return Err(AppError::from_str(
                "file",
                format!("invalid line: {}", line).as_str(),
            ));
        }
    }

    if let Some(day_entry) = current_day {
        day_entries.push_back(day_entry);
    }

    Ok(day_entries)
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

        assert_eq!(parse_time_ranges(time_range_str).unwrap(), expected);
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
        assert_eq!(parse_time_line(line).unwrap(), expected);
    }

    #[test]
    fn test_parse_file() {
        let file_content = "Date: Thursday 04/03/2025\n\nabc,xyz: 0800-1200,1300-1310,1318-1708\ndef,uvw: 1200-1300\n";
        let file_path = "test_file.txt";
        std::fs::write(file_path, file_content).unwrap();

        let expected = vector!(DayEntry::new2(
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
                ),
                ProjectTimes::new("def", "uvw", &vector!(time_range(12, 0, 13, 0),),),
            ),
        ));

        let result = parse_file(file_path).unwrap();

        assert_eq!(result, expected);

        std::fs::remove_file(file_path).unwrap(); // Clean up test file
    }
}
