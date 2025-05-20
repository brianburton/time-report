extern crate scopeguard;

mod append;
mod core;
mod menu;
mod model;
mod parse;
mod random;
mod report;
mod watch;

use core::AppError;
use crossterm::style::Stylize;
use im::{Vector, vector};
use itertools::Itertools;
use menu::{Menu, MenuItem};
use model::{Date, DateRange, DayEntry};
use std::env;
use std::env::Args;
use std::process::exit;

fn command_append(args: &mut Args) -> Result<(), AppError> {
    let (filename, all_day_entries) = load_file(args)?;
    let date = Date::today();
    append::validate_date(&all_day_entries, date)?;

    let min_date = date.minus_days(30)?;
    let recent_projects = append::recent_projects(&all_day_entries, min_date, 5);
    append::append_to_file(filename.as_str(), date, recent_projects)
}

fn command_random(args: &mut Args) -> Result<(), AppError> {
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

fn command_report(args: &mut Args) -> Result<(), AppError> {
    let (_, all_day_entries) = load_file(args)?;
    let dates = load_dates(args)?();
    println!("Reporting from {} to {}", dates.first(), dates.last());

    let lines = report::create_report(dates, &all_day_entries)?;
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}

fn command_watch(args: &mut Args) -> Result<(), AppError> {
    let filename = get_filename(args)?;
    let dates = load_dates(args)?;

    watch::watch_and_report(filename.as_str(), dates.as_ref())?;
    Ok(())
}

fn load_file(args: &mut Args) -> Result<(String, Vector<DayEntry>), AppError> {
    let filename = get_filename(args)?;

    println!("Loading {}...", filename);
    let (all_day_entries, warnings) = parse::parse_file(&filename)?;
    warnings.iter().for_each(|w| eprintln!("warning: {w}"));
    println!("Loaded {} dates from {}", all_day_entries.len(), filename);
    Ok((filename, all_day_entries))
}

fn get_filename(args: &mut Args) -> Result<String, AppError> {
    let filename = args
        .next()
        .ok_or_else(|| AppError::from_str("load_file", "usage: missing file name"))?;
    Ok(filename)
}

fn load_dates(args: &mut Args) -> Result<Box<dyn Fn() -> DateRange>, AppError> {
    let first_date = args.next().map(|s| Date::parse(&s)).transpose()?;
    let last_date = args.next().map(|s| Date::parse(&s)).transpose()?;
    let dates_fn: Box<dyn Fn() -> DateRange> = match (first_date, last_date) {
        (Some(first), Some(last)) => Box::new(move || DateRange::new(first, last)),
        (Some(date), None) => Box::new(move || date.semimonth_for_date()),
        _ => Box::new(|| Date::today().semimonth_for_date()),
    };
    Ok(dates_fn)
}

#[derive(Copy, Clone)]
enum MenuValue {
    Append,
    Reload,
    Quit,
}

fn main() -> Result<(), AppError> {
    // let menu_items = vector!(
    //     MenuItem::new(MenuValue::Append, "Append", "Add current date to the file."),
    //     MenuItem::new(MenuValue::Reload, "Reload", "Force reload of file."),
    //     MenuItem::new(MenuValue::Quit, "Quit", "Quit the program.")
    // );
    // let mut menu = Menu::new(menu_items.clone());
    // for _ in &menu_items {
    //     println!("{}", menu.render());
    //     println!("{}", menu.description().dark_yellow());
    //     menu.right();
    // }
    // for _ in &menu_items {
    //     menu.left();
    //     println!("{}", menu.render());
    //     println!("{}", menu.description().dark_yellow());
    // }
    // exit(1);
    let mut args = env::args();
    let command = args
        .nth(1)
        .ok_or_else(|| AppError::from_str("main", "usage: missing command"))?;

    match command.as_str() {
        "append" => command_append(&mut args),
        "random" => command_random(&mut args),
        "report" => command_report(&mut args),
        "watch" => command_watch(&mut args),
        _ => Err(AppError::from_str(
            "main",
            format!("usage: invalid command {}", command.as_str()).as_str(),
        ))?,
    }
}
