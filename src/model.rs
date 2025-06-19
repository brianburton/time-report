use std::fmt::Display;

use crate::core::parse_capture_group;
use anyhow::{Result, anyhow};
use chrono::Datelike;
use derive_getters::Getters;
use im::{HashSet, OrdSet, Vector, hashset, vector};
use lazy_static::lazy_static;
use regex::Regex;

#[cfg(test)]
mod tests;

lazy_static! {
    static ref TIME_RE: Regex = Regex::new(r"(\d{2})(\d{2})").unwrap();
    static ref DATE_RE: Regex = Regex::new(r"(\d{2})/(\d{2})/(\d{4})").unwrap();
    static ref LONG_MONTHS: HashSet<u8> = hashset!(1, 3, 5, 7, 8, 10, 12);
    static ref SHORT_MONTHS: HashSet<u8> = hashset!(4, 6, 9, 11);
    static ref DAY_ABBREVS: Vector<String> = vector!(
        "MON".to_string(),
        "TUE".to_string(),
        "WED".to_string(),
        "THU".to_string(),
        "FRI".to_string(),
        "SAT".to_string(),
        "SUN".to_string(),
    );
    static ref DAY_NAMES: Vector<String> = vector!(
        "Monday".to_string(),
        "Tuesday".to_string(),
        "Wednesday".to_string(),
        "Thursday".to_string(),
        "Friday".to_string(),
        "Saturday".to_string(),
        "Sunday".to_string(),
    );
}

pub const MIN_YEAR: u16 = 1973;
pub const MAX_YEAR: u16 = 2300;

/// Current time of day at minute resolution.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Copy, Clone)]
pub struct Time {
    minute: u16,
}

/// Displays the time as HHMM (note no colon between them).
impl Display for Time {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}{:02}", self.hour(), self.minute())
    }
}

impl Time {
    pub fn new(hour: u16, minute: u16) -> Result<Time> {
        if !is_valid_time(hour, minute) {
            Err(anyhow!("Time::new: not a valid time"))
        } else {
            let minute = hour * 60 + minute;
            Ok(Time { minute })
        }
    }

    pub fn parse(text: &str) -> Result<Time> {
        let re_captures = TIME_RE.captures(text);
        let h: u16 = parse_capture_group("Time::parse:hour", text, &re_captures, 1)?;
        let m: u16 = parse_capture_group("Time::parse:minute", text, &re_captures, 2)?;
        Self::new(h, m)
    }

    pub fn hour(&self) -> u16 {
        self.minute / 60
    }

    pub fn minute(&self) -> u16 {
        self.minute % 60
    }

    pub fn minute_of_day(&self) -> u16 {
        self.minute
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Getters, Copy, Hash)]
pub struct Date {
    year: u16,
    month: u8,
    day: u8,
}

impl Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}/{:02}/{:04}", self.month, self.day, self.year)
    }
}

impl Date {
    pub fn new(year: u16, month: u8, day: u8) -> Result<Date> {
        if !is_valid_date(year, month, day) {
            Err(anyhow!(format!(
                "Date::new: not a valid date: {}/{}/{}",
                month, day, year
            )))
        } else {
            Ok(Date { year, month, day })
        }
    }

    pub fn parse(text: &str) -> Result<Date> {
        let re_captures = DATE_RE.captures(text);
        let m: u8 = parse_capture_group("Date::parse:month", text, &re_captures, 1)?;
        let d: u8 = parse_capture_group("Date::parse:day", text, &re_captures, 2)?;
        let y: u16 = parse_capture_group("Date::parse:year", text, &re_captures, 3)?;
        Self::new(y, m, d)
    }

    pub fn today() -> Date {
        let d = chrono::Local::now();
        Date {
            year: d.year() as u16,
            month: d.month() as u8,
            day: d.day() as u8,
        }
    }

    pub fn min_date() -> Date {
        Date {
            year: MIN_YEAR,
            month: 1,
            day: 1,
        }
    }

    pub fn is_monday(&self) -> bool {
        self.day_num() % 7 == 0
    }

    pub fn is_sunday(&self) -> bool {
        self.day_num() % 7 == 6
    }

    pub fn is_weekend(&self) -> bool {
        let day_of_week = self.day_num() % 7;
        day_of_week == 5 || day_of_week == 6
    }

    pub fn is_weekday(&self) -> bool {
        !self.is_weekend()
    }

    pub fn this_monday(&self) -> Result<Date> {
        if self.is_monday() {
            Ok(*self)
        } else {
            self.prev_monday()
        }
    }

    pub fn this_sunday(&self) -> Result<Date> {
        if self.is_sunday() {
            Ok(*self)
        } else {
            self.next_monday()?.prev()
        }
    }

    pub fn prev_monday(&self) -> Result<Date> {
        let days_past = (self.day_num() % 7) as u8;
        let days_offset = if days_past == 0 { 7 } else { days_past };
        if days_offset < self.day {
            Date::new(self.year, self.month, self.day - days_offset)
        } else if self.month > 1 {
            Date::new(
                self.year,
                self.month - 1,
                days_in_month(self.year, self.month - 1) + self.day - days_offset,
            )
        } else {
            Date::new(self.year - 1, 12, 31 + self.day - days_offset)
        }
    }

    pub fn next_monday(&self) -> Result<Date> {
        let days_past = (self.day_num() % 7) as u8;
        let days_offset = 7 - days_past;
        let days_remaining = days_in_month(self.year, self.month) - self.day;
        if days_offset < days_remaining {
            Date::new(self.year, self.month, self.day + days_offset)
        } else if self.month < 12 {
            Date::new(self.year, self.month + 1, days_offset - days_remaining)
        } else {
            Date::new(self.year + 1, 1, days_offset - days_remaining)
        }
    }

    pub fn semimonth_for_date(&self) -> DateRange {
        if self.day <= 15 {
            DateRange::new(
                Date {
                    year: self.year,
                    month: self.month,
                    day: 1,
                },
                Date {
                    year: self.year,
                    month: self.month,
                    day: 15,
                },
            )
        } else {
            DateRange::new(
                Date {
                    year: self.year,
                    month: self.month,
                    day: 16,
                },
                Date {
                    year: self.year,
                    month: self.month,
                    day: days_in_month(self.year, self.month),
                },
            )
        }
    }

    pub fn day_abbrev(&self) -> String {
        DAY_ABBREVS[(self.day_num() % 7) as usize].clone()
    }

    pub fn day_name(&self) -> String {
        DAY_NAMES[(self.day_num() % 7) as usize].clone()
    }

    pub fn day_num(&self) -> u32 {
        day_number(self.year, self.month, self.day)
    }

    pub fn week_num(&self) -> u32 {
        self.day_num() / 7
    }

    pub fn prev(&self) -> Result<Date> {
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

    pub fn next(&self) -> Result<Date> {
        if self.day < days_in_month(self.year, self.month) {
            Date::new(self.year, self.month, self.day + 1)
        } else if self.month < 12 {
            Date::new(self.year, self.month + 1, 1)
        } else {
            Date::new(self.year + 1, 1, 1)
        }
    }

    pub(crate) fn minus_days(&self, days: i32) -> Result<Date> {
        let mut d = *self;
        let mut r = days;
        while r > 0 {
            d = d.prev()?;
            r -= 1;
        }
        Ok(d)
    }
}

pub struct DateIter {
    cur: Option<Date>,
    last: Date,
}

impl Iterator for DateIter {
    type Item = Date;

    fn next(&mut self) -> Option<Self::Item> {
        match self.cur {
            Some(d) => {
                self.cur = match d.next().ok() {
                    Some(dd) => {
                        if dd <= self.last {
                            Some(dd)
                        } else {
                            None
                        }
                    }
                    None => None,
                };
                Some(d)
            }
            None => None,
        }
    }
}

#[derive(Debug, Getters, Clone, Copy, PartialEq)]
pub struct DateRange {
    first: Date,
    last: Date,
}

impl DateRange {
    pub fn new(first: Date, last: Date) -> DateRange {
        DateRange { first, last }
    }

    pub fn iter(&self) -> DateIter {
        DateIter {
            cur: Some(self.first),
            last: self.last,
        }
    }

    pub fn contains(&self, d: &Date) -> bool {
        self.first <= *d && *d <= self.last
    }

    pub fn as_full_weeks(&self) -> Result<DateRange> {
        let first = self.first.this_monday()?;
        let last = self.last.this_sunday()?;
        Ok(DateRange { first, last })
    }
}

impl Display for DateRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}-{}", self.first, self.last)
    }
}

impl IntoIterator for DateRange {
    type Item = Date;
    type IntoIter = DateIter;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy, Getters)]
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
    pub fn new(from: Time, to: Time) -> Result<TimeRange> {
        if from >= to {
            Err(anyhow!("TimeRange::new: out of order time range"))
        } else {
            Ok(TimeRange { from, to })
        }
    }

    pub fn distinct(a: &TimeRange, b: &TimeRange) -> bool {
        a.to <= b.from || a.from >= b.to
    }

    pub fn duration(&self) -> u16 {
        self.to.minute_of_day() - self.from.minute_of_day()
    }
}

fn find_overlapping_time_ranges(time_ranges: &Vector<TimeRange>) -> OrdSet<TimeRange> {
    let mut conflicts = OrdSet::new();
    let mut visited = OrdSet::new();
    time_ranges.iter().for_each(|candidate| {
        visited.iter().for_each(|checked| {
            if !TimeRange::distinct(candidate, checked) {
                conflicts.insert(*checked);
                conflicts.insert(*candidate);
            }
        });
        visited.insert(*candidate);
    });
    conflicts
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash, Getters)]
pub struct Project {
    client: String,
    code: String,
}

impl Project {
    pub fn new(client: &str, code: &str) -> Project {
        Project {
            client: client.to_string(),
            code: code.to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Getters)]
pub struct ProjectTimes {
    project: Project,
    time_ranges: Vector<TimeRange>,
}

impl ProjectTimes {
    pub fn new(project: Project, time_ranges: &Vector<TimeRange>) -> Result<ProjectTimes> {
        let mut sorted = time_ranges.clone();
        let conflicts = find_overlapping_time_ranges(time_ranges);
        if !conflicts.is_empty() {
            let detail = format!(
                "conflicting time ranges: client: {} project: {} conflicts: {}",
                project.client,
                project.code,
                ordset_to_string(&conflicts)
            );
            return Err(anyhow!("ProjectTimes::new: {}", detail));
        }
        sorted.sort();
        Ok(ProjectTimes {
            project: project.clone(),
            time_ranges: sorted,
        })
    }
}

#[derive(Debug, PartialEq, Clone, Getters)]
pub struct DayEntry {
    date: Date,
    projects: Vector<ProjectTimes>,
    line_number: u32,
}

impl DayEntry {
    pub fn new(date: Date, projects: &Vector<ProjectTimes>, line_number: u32) -> Self {
        DayEntry {
            date,
            projects: projects.clone(),
            line_number,
        }
    }
}

fn is_valid_time(hour: u16, minute: u16) -> bool {
    hour < 24 && minute < 60
}

fn is_leap_year(year: u16) -> bool {
    (year % 4 == 0) && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: u16, month: u8) -> u8 {
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

fn day_number(year: u16, month: u8, day: u8) -> u32 {
    let past_year_days = (MIN_YEAR..year).fold(0, |s: u32, y: u16| s + days_in_year(y) as u32);
    let past_month_days: u32 = (1..month).fold(0, |s, m| s + days_in_month(year, m) as u32);
    past_year_days + past_month_days + day as u32 - 1
}

fn is_valid_date(year: u16, month: u8, day: u8) -> bool {
    (MIN_YEAR..=MAX_YEAR).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
}

fn ordset_to_string<T: Clone + Ord + Display>(values: &OrdSet<T>) -> String {
    let mut x = String::new();
    x.push('[');
    values.iter().for_each(|i| {
        if x.len() > 1 {
            x.push(',');
        }
        x.push_str(i.to_string().as_str());
    });
    x.push(']');
    x
}
