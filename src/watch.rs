use crate::core::AppError;
use crate::model::DateRange;
use crate::parse;
use crate::report;
use crossterm::{QueueableCommand, cursor, terminal};
use std::fs;
use std::io::{Write, stdout};
use std::thread;
use std::time::{Duration, SystemTime, SystemTimeError};

pub fn watch_and_report(filename: &str, dates: DateRange) -> Result<(), AppError> {
    let sleep_time = Duration::from_millis(200);
    let update_delay_millis = 500;
    let mut next_update_millis: u128 = 0;
    loop {
        let current_file_millis = get_last_modified(filename)?;
        if current_file_millis >= next_update_millis {
            next_update_millis = current_file_millis + update_delay_millis;
            let result = parse::parse_file(filename);
            if let Ok((all_day_entries, warnings)) = result {
                clear_screen()?;
                warnings.iter().for_each(|w| println!("warning: {w}"));
                let lines = report::create_report(dates, &all_day_entries)?;
                for line in lines {
                    println!("{}", line);
                }
            } else {
                clear_screen()?;
                println!(
                    "error reading file: filename={} error={}",
                    filename,
                    result.unwrap_err()
                );
            }
        }
        thread::sleep(sleep_time);
    }
}

fn get_last_modified(filename: &str) -> Result<u128, AppError> {
    let io_err =
        |detail: &str, e: std::io::Error| AppError::from_error("get_last_modified", detail, e);
    let time_err =
        |detail: &str, e: SystemTimeError| AppError::from_error("get_last_modified", detail, e);
    let metadata = fs::metadata(filename).map_err(|e| io_err("metadata", e))?;
    let modified = metadata.modified().map_err(|e| io_err("modified", e))?;
    let millis = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|e| time_err("duration_since", e))?
        .as_millis();
    Ok(millis)
}

fn clear_screen() -> Result<(), AppError> {
    let io_err = |detail: &str, e: std::io::Error| AppError::from_error("clear_screen", detail, e);
    let mut out = stdout();
    out.queue(terminal::Clear(terminal::ClearType::All))
        .map_err(|e| io_err("Clear", e))?;
    out.queue(cursor::MoveTo(0, 0))
        .map_err(|e| io_err("MoveTo", e))?;
    out.flush().map_err(|e| io_err("flush", e))?;
    Ok(())
}
