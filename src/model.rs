use crate::core::{AppError, parse_digits};
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
    pub fn min_date() -> Date {
        Date {
            year: MIN_YEAR,
            month: 1,
            day: 1,
        }
    }

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
    pub fn new(date: Date, projects: &Vector<ProjectTimes>) -> Self {
        DayEntry {
            date,
            projects: projects.clone(),
        }
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
