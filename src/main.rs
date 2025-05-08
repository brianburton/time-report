extern crate scopeguard;

mod append;
mod core;
mod model;
mod parse;
mod random;
mod report;

use core::AppError;
use im::Vector;
use model::{Date, DateRange, DayEntry};
use std::env;
use std::env::Args;

fn command_append(args: &mut Args) -> Result<(), AppError> {
    let (filename, all_day_entries) = load_file(args)?;
    let date = Date::today();
    append::validate_date(&all_day_entries, date)?;

    let min_date = date.minus_days(30)?;
    let recent_projects = append::recent_projects(&all_day_entries, min_date, 5);
    append::append_to_file(filename.as_str(), date, recent_projects)
}

fn command_random(args: &mut Args) -> Result<(), AppError> {
    let dates = load_dates(args)?;
    let mut rnd = random::Random::new();
    let day_entries = random::random_day_entries(&mut rnd, dates);
    for (date_count, de) in day_entries.into_iter().enumerate() {
        if date_count > 0 {
            println!()
        }
        println!("Date: {} {}", de.date().day_name(), de.date());
        for pt in de.projects() {
            print!("{},{}: ", pt.project().client(), pt.project().code());
            for (time_count, t) in pt.time_ranges().into_iter().enumerate() {
                if time_count > 0 {
                    print!(",")
                }
                print!("{}-{}", t.from(), t.to());
            }
            println!()
        }
    }
    Ok(())
}

fn command_report(args: &mut Args) -> Result<(), AppError> {
    let (_, all_day_entries) = load_file(args)?;
    let dates = load_dates(args)?;
    println!("Reporting from {} to {}", dates.first(), dates.last());

    let lines = report::create_report(dates, &all_day_entries)?;
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}

fn load_file(args: &mut Args) -> Result<(String, Vector<DayEntry>), AppError> {
    let filename = args
        .next()
        .ok_or_else(|| AppError::from_str("load_file", "usage: missing file name"))?;

    println!("Loading {}...", filename);
    let (all_day_entries, warnings) = parse::parse_file(&filename)?;
    warnings.iter().for_each(|w| eprintln!("warning: {w}"));
    println!("Loaded {} dates from {}", all_day_entries.len(), filename);
    Ok((filename, all_day_entries))
}

fn load_dates(args: &mut Args) -> Result<DateRange, AppError> {
    let first_date_str = args.next();
    let last_date_str = args.next();
    let first_date_str = first_date_str.unwrap_or_else(|| Date::today().to_string());
    let date_range = match last_date_str {
        Some(s) => DateRange::new(Date::parse(&first_date_str)?, Date::parse(&s)?),
        None => Date::parse(&first_date_str)?.semimonth_for_date(),
    };
    Ok(date_range)
}

fn main() -> Result<(), AppError> {
    let mut args = env::args();
    let command = args
        .nth(1)
        .ok_or_else(|| AppError::from_str("main", "usage: missing command"))?;

    match command.as_str() {
        "append" => command_append(&mut args),
        "random" => command_random(&mut args),
        "report" => command_report(&mut args),
        _ => Err(AppError::from_str(
            "main",
            format!("usage: invalid command {}", command.as_str()).as_str(),
        ))?,
    }
}
