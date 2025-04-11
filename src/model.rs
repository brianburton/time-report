use std::{fmt::Display, ops::Range};

use crate::core::{AppError, parse_digits};
use chrono::Datelike;
use derive_getters::Getters;
use im::{HashSet, OrdSet, Vector, hashset, vector};
use lazy_static::lazy_static;
use regex::Regex;

lazy_static! {
    static ref TIME_RE: Regex = Regex::new(r"(\d{2})(\d{2})").unwrap();
    static ref DATE_RE: Regex = Regex::new(r"(\d{2})/(\d{2})/(\d{4})").unwrap();
    static ref LONG_MONTHS: HashSet<u16> = hashset!(1, 3, 5, 7, 8, 10, 12);
    static ref SHORT_MONTHS: HashSet<u16> = hashset!(4, 6, 9, 11);
    static ref DAY_ABBREVS: Vector<String> = vector!(
        "MON".to_string(),
        "TUE".to_string(),
        "WED".to_string(),
        "THU".to_string(),
        "FRI".to_string(),
        "SAT".to_string(),
        "SUN".to_string(),
    );
}

pub const MIN_YEAR: u16 = 1973;
pub const MAX_YEAR: u16 = 2300;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone)]
pub struct Time {
    minute: u16,
}

impl Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}{:02}", self.hour(), self.minute())
    }
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

impl Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}/{:02}/{:04}", self.month, self.day, self.year)
    }
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

    pub fn today() -> Date {
        let d = chrono::Local::now();
        Date {
            year: d.year() as u16,
            month: d.month() as u16,
            day: d.day() as u16,
        }
    }

    pub fn min_date() -> Date {
        Date {
            year: MIN_YEAR,
            month: 1,
            day: 1,
        }
    }

    pub fn semimonth_for_date(&self) -> Range<Date> {
        if self.day <= 15 {
            Date {
                year: self.year,
                month: self.month,
                day: 1,
            }..Date {
                year: self.year,
                month: self.month,
                day: 16,
            }
        } else {
            Date {
                year: self.year,
                month: self.month,
                day: 15,
            }..Date {
                year: self.year,
                month: self.month,
                day: days_in_month(self.year, self.month) + 1,
            }
        }
    }

    pub fn day_abbrev(&self) -> String {
        DAY_ABBREVS[(self.day_num() % 7) as usize].clone()
    }

    pub fn day_num(&self) -> u32 {
        day_number(self.year, self.month, self.day)
    }

    pub fn week_num(&self) -> u32 {
        self.day_num() / 7
    }

    pub fn prev(&self) -> Result<Date, AppError> {
        if self.day > 1 {
            Date::new(self.year, self.month, self.day - 1)
        } else if self.month > 1 {
            Date::new(
                self.year,
                self.month - 1,
                days_in_month(self.year, self.month - 1),
            )
        } else {
            Date::new(self.year - 1, 12, 31)
        }
    }

    pub fn next(&self) -> Result<Date, AppError> {
        if self.day < days_in_month(self.year, self.month) {
            Date::new(self.year, self.month, self.day + 1)
        } else if self.month < 12 {
            Date::new(self.year, self.month + 1, 1)
        } else {
            Date::new(self.year + 1, self.month, self.day)
        }
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Getters)]
pub struct TimeRange {
    from: Time,
    to: Time,
}

impl Display for TimeRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.from, self.to)
    }
}

impl TimeRange {
    pub fn new(from: Time, to: Time) -> Result<TimeRange, AppError> {
        if from >= to {
            Err(AppError::from_str("model", "out of order time range"))
        } else {
            Ok(TimeRange { from, to })
        }
    }

    pub fn distinct(a: &TimeRange, b: &TimeRange) -> bool {
        a.to <= b.from || a.from >= b.to
    }
}

fn find_overlapping_time_ranges(time_ranges: &Vector<TimeRange>) -> OrdSet<TimeRange> {
    let mut conflicts = OrdSet::new();
    let mut visited = OrdSet::new();
    time_ranges.iter().for_each(|candidate| {
        visited.iter().for_each(|checked| {
            if !TimeRange::distinct(candidate, checked) {
                conflicts.insert(checked.clone());
                conflicts.insert(candidate.clone());
            }
        });
        visited.insert(candidate.clone());
    });
    conflicts
}

#[derive(Debug, PartialEq, Clone, Getters)]
pub struct ProjectTimes {
    client: String,
    project: String,
    time_ranges: Vector<TimeRange>,
}

impl ProjectTimes {
    pub fn new(
        client: &str,
        project: &str,
        time_ranges: &Vector<TimeRange>,
    ) -> Result<ProjectTimes, AppError> {
        let mut sorted = time_ranges.clone();
        let conflicts = find_overlapping_time_ranges(time_ranges);
        if !conflicts.is_empty() {
            let detail = format!(
                "conflicting time ranges: client: {} project: {} conflicts: {}",
                client,
                project,
                ordset_to_string(&conflicts)
            );
            return Err(AppError::from_str("projects", detail.as_str()));
        }
        sorted.sort();
        Ok(ProjectTimes {
            client: client.to_string(),
            project: project.to_string(),
            time_ranges: sorted,
        })
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

fn day_number(year: u16, month: u16, day: u16) -> u32 {
    let past_year_days = (MIN_YEAR..year).fold(0, |s: u32, y: u16| s + days_in_year(y) as u32);
    let past_month_days: u32 = (1..month).fold(0, |s, m| s + days_in_month(year, m) as u32);
    past_year_days + past_month_days + day as u32 - 1
}

fn is_valid_date(year: u16, month: u16, day: u16) -> bool {
    (MIN_YEAR..=MAX_YEAR).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
}

fn vector_to_string<T: Clone + Display>(values: &Vector<T>) -> String {
    let mut x = String::new();
    x.push_str("[");
    values.iter().for_each(|i| {
        if x.len() > 1 {
            x.push_str(",");
        }
        x.push_str(i.to_string().as_str());
    });
    x.push_str("]");
    x
}

fn ordset_to_string<T: Clone + Ord + Display>(values: &OrdSet<T>) -> String {
    let mut x = String::new();
    x.push_str("[");
    values.iter().for_each(|i| {
        if x.len() > 1 {
            x.push_str(",");
        }
        x.push_str(i.to_string().as_str());
    });
    x.push_str("]");
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use im::{ordset, vector};

    fn date(y: u16, m: u16, d: u16) -> Date {
        Date::new(y, m, d).unwrap()
    }

    fn time(h: u16, m: u16) -> Time {
        Time::new(h, m).unwrap()
    }

    fn time_range(h1: u16, m1: u16, h2: u16, m2: u16) -> TimeRange {
        TimeRange::new(time(h1, m1), time(h2, m2)).unwrap()
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
    fn test_date_names() {
        assert_eq!("MON", Date::min_date().day_abbrev());
        assert_eq!("SUN", date(2025, 4, 6).day_abbrev());
        assert_eq!("MON", date(2025, 4, 7).day_abbrev());
        assert_eq!("TUE", date(2025, 4, 8).day_abbrev());
        assert_eq!("WED", date(2025, 4, 9).day_abbrev());
        assert_eq!("THU", date(2025, 4, 10).day_abbrev());
        assert_eq!("FRI", date(2025, 4, 11).day_abbrev());
        assert_eq!("SAT", date(2025, 4, 12).day_abbrev());
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
        assert_eq!(0, day_number(MIN_YEAR, 1, 1));
        assert_eq!(365, day_number(MIN_YEAR + 1, 1, 1));

        let base = day_number(2000, 12, 31);
        assert_eq!(10226, base);

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

    #[test]
    fn test_time_range_distinct() {
        // separated
        assert!(TimeRange::distinct(
            &time_range(1, 0, 3, 0),
            &time_range(5, 0, 7, 0),
        ));
        assert!(TimeRange::distinct(
            &time_range(5, 0, 7, 0),
            &time_range(1, 0, 3, 0),
        ));

        // adjacent
        assert!(TimeRange::distinct(
            &time_range(1, 0, 3, 0),
            &time_range(3, 0, 5, 0),
        ));
        assert!(TimeRange::distinct(
            &time_range(3, 0, 5, 0),
            &time_range(1, 0, 3, 0),
        ));

        // overlapping
        assert!(!TimeRange::distinct(
            &time_range(1, 0, 3, 0),
            &time_range(2, 59, 5, 0),
        ));
        assert!(!TimeRange::distinct(
            &time_range(3, 0, 5, 0),
            &time_range(1, 0, 3, 1),
        ));
        assert!(!TimeRange::distinct(
            &time_range(3, 0, 5, 0),
            &time_range(3, 1, 4, 59),
        ));
        assert!(!TimeRange::distinct(
            &time_range(3, 0, 5, 0),
            &time_range(3, 0, 4, 59),
        ));
        assert!(!TimeRange::distinct(
            &time_range(3, 0, 5, 0),
            &time_range(3, 1, 5, 0),
        ));
    }

    #[test]
    fn test_find_overlapping_time_ranges() {
        let t13 = time_range(1, 0, 3, 0);
        let t24 = time_range(2, 0, 4, 0);
        let t34 = time_range(3, 0, 4, 0);
        let t35 = time_range(3, 0, 5, 0);
        let t56 = time_range(5, 0, 6, 0);
        let empty: Vector<TimeRange> = vector!();
        let no_match: OrdSet<TimeRange> = ordset!();

        assert_eq!(no_match, find_overlapping_time_ranges(&empty));
        assert_eq!(
            no_match,
            find_overlapping_time_ranges(&vector!(t13.clone(), t35.clone()))
        );
        assert_eq!(
            ordset!(t13.clone(), t35.clone(), t24.clone()),
            find_overlapping_time_ranges(&vector!(t13.clone(), t35.clone(), t24.clone()))
        );
        assert_eq!(
            no_match,
            find_overlapping_time_ranges(&vector!(t13.clone(), t35.clone(), t56.clone()))
        );
        assert_eq!(
            ordset!(t34.clone(), t35.clone()),
            find_overlapping_time_ranges(&vector!(
                t13.clone(),
                t34.clone(),
                t35.clone(),
                t56.clone()
            ))
        );
    }

    #[test]
    fn test_displays() {
        assert_eq!("0102", time(1, 2).to_string());
        assert_eq!("2359", time(23, 59).to_string());
        assert_eq!("0102-2359", time_range(1, 2, 23, 59).to_string());
        assert_eq!("01/02/1995", Date::new(1995, 1, 2).unwrap().to_string());
        assert_eq!("[1]", vector_to_string(&vector!(1)));
        assert_eq!("[1,2]", vector_to_string(&vector!(1, 2)));
        assert_eq!("[1,2]", ordset_to_string(&ordset!(2, 1)));
    }
}
