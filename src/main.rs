mod core;
mod model;
mod parse;
use std::env;

fn main() {
    env::args().skip(1).for_each(|filename| {
        println!("Loading {}...", filename.as_str());
        let (dates, warnings) = parse::parse_file(filename.as_str()).unwrap();
        warnings.iter().for_each(|w| println!("{w}"));
        println!("Loaded {} dates from {}", dates.len(), filename.as_str());
    });
}
