extern crate scopeguard;

mod append;
mod core;
mod model;
mod parse;
mod random;
mod report;
mod watch;

use anyhow::{Result, anyhow};
use im::Vector;
use model::{Date, DateRange, DayEntry};
use std::env;
use std::env::Args;

fn command_append(args: &mut Args) -> Result<()> {
    let (filename, all_day_entries) = load_file(args)?;
    let date = Date::today();
    append::validate_date(&all_day_entries, date)?;

    let min_date = date.minus_days(30)?;
    let recent_projects = append::recent_projects(&all_day_entries, min_date, 5);
    append::append_to_file(filename.as_str(), date, &recent_projects)
}

fn command_random(args: &mut Args) -> Result<()> {
    let dates = load_dates(args)?();
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

fn command_report(args: &mut Args) -> Result<()> {
    let (_, all_day_entries) = load_file(args)?;
    let dates = load_dates(args)?();
    println!("Reporting from {} to {}", dates.first(), dates.last());

    let day_entries = report::day_entries_in_range(&dates, &all_day_entries);
    let lines = report::create_report(dates, &day_entries)?;
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}

fn command_watch(args: &mut Args) -> Result<()> {
    let filename = get_filename(args)?;
    let dates = load_dates(args)?;

    watch::watch_and_report(filename.as_str(), dates.as_ref())?;
    Ok(())
}

fn load_file(args: &mut Args) -> Result<(String, Vector<DayEntry>)> {
    let filename = get_filename(args)?;

    println!("Loading {}...", filename);
    let (all_day_entries, warnings) = parse::parse_file(&filename)?;
    warnings.iter().for_each(|w| eprintln!("warning: {w}"));
    println!("Loaded {} dates from {}", all_day_entries.len(), filename);
    Ok((filename, all_day_entries))
}

fn get_filename(args: &mut Args) -> Result<String> {
    let filename = args
        .next()
        .ok_or_else(|| anyhow!("load_file: usage: missing file name"))?;
    Ok(filename)
}

fn load_dates(args: &mut Args) -> Result<Box<dyn Fn() -> DateRange>> {
    let first_date = args.next().map(|s| Date::parse(&s)).transpose()?;
    let last_date = args.next().map(|s| Date::parse(&s)).transpose()?;
    let dates_fn: Box<dyn Fn() -> DateRange> = match (first_date, last_date) {
        (Some(first), Some(last)) => Box::new(move || DateRange::new(first, last)),
        (Some(date), None) => Box::new(move || date.semimonth_for_date()),
        _ => Box::new(|| Date::today().semimonth_for_date()),
    };
    Ok(dates_fn)
}

fn main() -> Result<()> {
    let mut args = env::args();
    let command = args
        .nth(1)
        .ok_or_else(|| anyhow!("main: usage: missing command"))?;

    match command.as_str() {
        "append" => command_append(&mut args),
        "random" => command_random(&mut args),
        "report" => command_report(&mut args),
        "watch" => command_watch(&mut args),
        _ => Err(anyhow!("main: usage: invalid command {}", command.as_str()))?,
    }
}
