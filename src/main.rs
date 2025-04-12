mod core;
mod model;
mod parse;
mod report;
use std::env;

fn main() -> Result<(), core::AppError> {
    let filename = env::args().skip(1).next().unwrap();
    println!("Loading {}...", filename.as_str());
    let (dates, warnings) = parse::parse_file(filename.as_str()).unwrap();
    warnings.iter().for_each(|w| println!("{w}"));
    println!("Loaded {} dates from {}", dates.len(), filename.as_str());
    let range = model::DateRange::new(
        model::Date::parse("04/01/2025")?,
        model::Date::parse("04/15/2025")?,
    );
    let lines = report::create_report(range, &dates)?;
    for line in lines {
        println!("{}", line);
    }
    Ok(())
}
