extern crate scopeguard;

mod append;
mod core;
mod model;
mod parse;
mod report;

use crate::append::append_to_file;
use crate::model::DayEntry;
use core::AppError;
use im::Vector;
use model::{Date, DateRange};
use std::env;

fn command_report(
    all_day_entries: &Vector<DayEntry>,
    first_date_str: Option<String>,
    last_date_str: Option<String>,
) -> Result<(), AppError> {
    let first_date_str = first_date_str.unwrap_or_else(|| Date::today().to_string());
    let dates = match last_date_str {
        Some(s) => DateRange::new(Date::parse(&first_date_str)?, Date::parse(&s)?),
        None => Date::parse(&first_date_str)?.semimonth_for_date(),
    };
    println!("Reporting from {} to {}", dates.first(), dates.last());

    let lines = report::create_report(dates, all_day_entries)?;
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}

fn command_append(all_day_entries: &Vector<DayEntry>, filename: &str) -> Result<(), AppError> {
    let date = Date::today();
    let min_date = date.minus_days(30)?;
    let recent_projects = append::recent_projects(all_day_entries, min_date, 5);
    append_to_file(filename, date, recent_projects)
}

fn main() -> Result<(), AppError> {
    let mut args = env::args();
    let command = args
        .nth(1)
        .ok_or_else(|| AppError::from_str("usage", "missing command"))?;

    let filename = args
        .next()
        .ok_or_else(|| AppError::from_str("usage", "missing file name"))?;

    println!("Loading {}...", filename);
    let (all_day_entries, warnings) = parse::parse_file(&filename)?;
    warnings.iter().for_each(|w| eprintln!("warning: {w}"));
    println!("Loaded {} dates from {}", all_day_entries.len(), filename);

    match command.as_str() {
        "report" => command_report(&all_day_entries, args.next(), args.next()),
        "append" => command_append(&all_day_entries, &filename),
        _ => Err(AppError::from_str("usage", "invalid command"))?,
    }
}
