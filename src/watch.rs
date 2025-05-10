use crate::core::AppError;
use crate::model::{Date, DateRange, DayEntry};
use crate::report;
use crate::watch::PollOutcome::{Changed, DoNothing, LoadFailed, Reloaded};
use crate::{append, parse};
use crossterm::event::KeyCode;
use crossterm::event::{Event, poll, read};
use crossterm::{QueueableCommand, cursor, terminal};
use im::Vector;
use scopeguard::defer;
use std::fs;
use std::io::{Write, stdout};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, SystemTimeError};

struct Tracker<'a> {
    filename: &'a str,
    next_update_millis: u128,
    poll_wait_duration: Duration,
    update_delay_millis: u128,
}

struct LoadedFile {
    day_entries: Vector<DayEntry>,
    warnings: Vector<String>,
}

enum PollOutcome {
    DoNothing,
    Quit,
    Reloaded(LoadedFile),
    Changed(LoadedFile),
    LoadFailed(AppError),
}

impl<'a> Tracker<'a> {
    fn new(filename: &'a str) -> Tracker<'a> {
        Tracker {
            filename,
            next_update_millis: 0,
            update_delay_millis: 500,
            poll_wait_duration: Duration::from_millis(200),
        }
    }

    fn next_command(&mut self) -> Result<PollOutcome, AppError> {
        let io_err =
            |detail: &str, e: std::io::Error| AppError::from_error("next_command", detail, e);
        if poll(self.poll_wait_duration).map_err(|e| io_err("poll", e))? {
            match read().map_err(|e| io_err("read", e))? {
                Event::Key(event) => match event.code {
                    KeyCode::Char('q') => Ok(PollOutcome::Quit),
                    KeyCode::Char('r') => {
                        let loaded = self.load(true);
                        match loaded {
                            Ok(Some(loaded)) => Ok(Reloaded(loaded)),
                            Ok(None) => Ok(DoNothing),
                            Err(e) => Ok(LoadFailed(e)),
                        }
                    }
                    KeyCode::Char('e') => {
                        let loaded = self.edit();
                        match loaded {
                            Ok(Some(loaded)) => Ok(Reloaded(loaded)),
                            Ok(None) => Ok(DoNothing),
                            Err(e) => Ok(LoadFailed(e)),
                        }
                    }
                    KeyCode::Char('a') => {
                        let loaded = self.append().and_then(|()| self.load(true));
                        match loaded {
                            Ok(Some(loaded)) => Ok(Reloaded(loaded)),
                            Ok(None) => Ok(DoNothing),
                            Err(e) => Ok(LoadFailed(e)),
                        }
                    }
                    _ => Ok(DoNothing),
                },
                Event::Resize(_, _) => {
                    let loaded = self.load(true);
                    match loaded {
                        Ok(Some(loaded)) => Ok(Reloaded(loaded)),
                        Ok(None) => Ok(DoNothing),
                        Err(e) => Ok(LoadFailed(e)),
                    }
                }
                _ => Ok(DoNothing),
            }
        } else {
            let loaded = self.load(false);
            match loaded {
                Ok(Some(loaded)) => Ok(Changed(loaded)),
                Ok(None) => Ok(DoNothing),
                Err(e) => Ok(LoadFailed(e)),
            }
        }
    }

    fn load(&mut self, reload: bool) -> Result<Option<LoadedFile>, AppError> {
        let current_file_millis = get_last_modified(self.filename)?;
        if current_file_millis < self.next_update_millis && !reload {
            return Ok(None);
        }
        self.next_update_millis = current_file_millis + self.update_delay_millis;
        let (day_entries, warnings) = parse::parse_file(self.filename)?;
        Ok(Some(LoadedFile {
            day_entries,
            warnings,
        }))
    }

    fn append(&mut self) -> Result<(), AppError> {
        let (day_entries, _) = parse::parse_file(self.filename)?;
        let date = Date::today();
        append::validate_date(&day_entries, date)?;

        let min_date = date.minus_days(30)?;
        let recent_projects = append::recent_projects(&day_entries, min_date, 5);
        append::append_to_file(self.filename, date, recent_projects)
    }

    fn edit(&mut self) -> Result<Option<LoadedFile>, AppError> {
        let io_err =
            |detail: &str, e: std::io::Error| AppError::from_error("watch_and_report", detail, e);
        let (day_entries, _) = parse::parse_file(self.filename)?;
        let line_number = day_entries
            .iter()
            .next_back()
            .map(|e| e.line_number())
            .unwrap_or(&0);

        let line_param = format!("+{}", line_number + 1);
        let status = Command::new("hx")
            .arg(line_param)
            .arg(self.filename)
            .spawn()
            .map_err(|e| io_err("spawn", e))?
            .wait()
            .map_err(|e| io_err("wait", e))?;
        if !status.success() {
            return Err(AppError::from_str("edit", "editor command failed"));
        }
        self.load(true)
    }
}

pub fn watch_and_report(filename: &str, dates: DateRange) -> Result<(), AppError> {
    let io_err =
        |detail: &str, e: std::io::Error| AppError::from_error("watch_and_report", detail, e);
    terminal::enable_raw_mode().map_err(|e| io_err("enable_raw_mode", e))?;
    defer! {
        _=terminal::disable_raw_mode();
    }
    let mut tracker = Tracker::new(filename);
    loop {
        let outcome = tracker.next_command()?;
        match outcome {
            PollOutcome::Quit => return Ok(()),
            PollOutcome::DoNothing => {
                thread::sleep(tracker.poll_wait_duration);
            }
            PollOutcome::Reloaded(loaded) => {
                print_file(&loaded, dates)?;
            }
            PollOutcome::Changed(loaded) => {
                print_file(&loaded, dates)?;
            }
            PollOutcome::LoadFailed(error) => {
                print_error(filename, error)?;
            }
        }
    }
}

fn print_error(filename: &str, error: AppError) -> Result<(), AppError> {
    clear_screen()?;
    println!(
        "error reading file: filename={} error={}\r",
        filename, error
    );
    Ok(())
}

fn print_file(file: &LoadedFile, dates: DateRange) -> Result<(), AppError> {
    clear_screen()?;
    file.warnings
        .iter()
        .for_each(|w| println!("warning: {w}\r"));
    let lines = report::create_report(dates, &file.day_entries)?;
    for line in lines {
        println!("{}\r", line);
    }
    Ok(())
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
