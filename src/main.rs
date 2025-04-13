mod core;
mod model;
mod parse;
mod report;
use core::AppError;
use std::env;

use model::{Date, DateRange};

fn main() -> Result<(), AppError> {
    let mut args = env::args();
    let filename = args
        .nth(1)
        .ok_or_else(|| AppError::from_str("usage", "missing file name"))?;

    println!("Loading {}...", filename.as_str());
    let (all_day_entries, warnings) = parse::parse_file(filename.as_str())?;
    warnings.iter().for_each(|w| eprintln!("warning: {w}"));
    println!(
        "Loaded {} dates from {}",
        all_day_entries.len(),
        filename.as_str()
    );

    let first_date_str = args.next().unwrap_or_else(|| Date::today().to_string());
    let last_date_str = args.next();
    let dates = match last_date_str {
        Some(s) => DateRange::new(Date::parse(&first_date_str)?, Date::parse(&s)?),
        None => Date::parse(&first_date_str)?.semimonth_for_date(),
    };
    println!("Reporting from {} to {}", dates.first(), dates.last());

    let lines = report::create_report(dates, &all_day_entries)?;
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}
