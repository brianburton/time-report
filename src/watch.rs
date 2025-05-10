use crate::core::AppError;
use crate::model::{Date, DateRange, DayEntry};
use crate::report;
use crate::{append, parse};
use crossterm::cursor::{Hide, Show};
use crossterm::event::KeyCode;
use crossterm::event::{Event, poll, read};
use crossterm::{QueueableCommand, cursor, execute, terminal};
use derive_getters::Getters;
use im::Vector;
use regex::Regex;
use scopeguard::defer;
use std::env;
use std::fs;
use std::io::{Write, stdout};
use std::process::Command;
use std::time::{Duration, SystemTime, SystemTimeError};

struct UI<'a> {
    filename: &'a str,
    last_update_millis: u128,
    loaded: LoadedFile,
    poll_wait_duration: Duration,
    update_delay_millis: u128,
}

#[derive(Clone, Getters)]
struct LoadedFile {
    day_entries: Vector<DayEntry>,
    warnings: Vector<String>,
}

enum UICommand {
    DoNothing,
    Quit,
    Report(LoadedFile),
    DisplayError(AppError),
}

impl LoadedFile {
    fn new(day_entries: &Vector<DayEntry>, warnings: &Vector<String>) -> Self {
        LoadedFile {
            day_entries: day_entries.clone(),
            warnings: warnings.clone(),
        }
    }

    fn empty() -> Self {
        LoadedFile {
            day_entries: Vector::new(),
            warnings: Vector::new(),
        }
    }
}

impl<'a> UI<'a> {
    fn new(filename: &'a str) -> UI<'a> {
        UI {
            filename,
            loaded: LoadedFile::empty(),
            last_update_millis: 0,
            update_delay_millis: 500,
            poll_wait_duration: Duration::from_millis(100),
        }
    }

    fn next_command(&mut self) -> Result<UICommand, AppError> {
        let io_err =
            |detail: &str, e: std::io::Error| AppError::from_error("next_command", detail, e);
        if poll(self.poll_wait_duration).map_err(|e| io_err("poll", e))? {
            match read().map_err(|e| io_err("read", e))? {
                Event::Key(event) => match event.code {
                    KeyCode::Char('q') => Ok(UICommand::Quit),
                    KeyCode::Char('r') | KeyCode::Enter => match self.load(true) {
                        Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                        Err(e) => Ok(UICommand::DisplayError(e)),
                    },
                    KeyCode::Char('e') => match self.edit() {
                        Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                        Err(e) => Ok(UICommand::DisplayError(e)),
                    },
                    KeyCode::Char('a') => match self.append() {
                        Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                        Err(e) => Ok(UICommand::DisplayError(e)),
                    },
                    _ => Ok(UICommand::DoNothing),
                },
                Event::Resize(_, _) => Ok(UICommand::Report(self.loaded.clone())),
                _ => Ok(UICommand::DoNothing),
            }
        } else {
            match self.load(false) {
                Ok((true, loaded)) => Ok(UICommand::Report(loaded)),
                Ok((false, _)) => Ok(UICommand::DoNothing),
                Err(e) => Ok(UICommand::DisplayError(e)),
            }
        }
    }

    fn load(&mut self, skip_delay: bool) -> Result<(bool, LoadedFile), AppError> {
        let current_file_millis = get_last_modified(self.filename)?;
        if current_file_millis == self.last_update_millis {
            return Ok((false, self.loaded.clone()));
        }
        let next_update_millis = self.last_update_millis + self.update_delay_millis;
        if current_file_millis < next_update_millis && !skip_delay {
            return Ok((false, self.loaded.clone()));
        }
        self.last_update_millis = current_file_millis;
        let (day_entries, warnings) = parse::parse_file(self.filename)?;
        self.loaded = LoadedFile::new(&day_entries, &warnings);
        Ok((true, self.loaded.clone()))
    }

    fn append(&mut self) -> Result<(bool, LoadedFile), AppError> {
        self.load(true)?;
        let date = Date::today();
        let day_entries = self.loaded.day_entries();
        append::validate_date(day_entries, date)?;

        let min_date = date.minus_days(30)?;
        let recent_projects = append::recent_projects(day_entries, min_date, 5);
        append::append_to_file(self.filename, date, recent_projects)?;
        self.load(true)
    }

    fn edit(&mut self) -> Result<(bool, LoadedFile), AppError> {
        let io_err = |detail: &str, e: std::io::Error| AppError::from_error("edit", detail, e);
        let line_number = self
            .loaded
            .day_entries()
            .iter()
            .next_back()
            .map(|e| e.line_number())
            .unwrap_or(&0);

        restore_term();
        defer! {
            _=init_term();
        }

        let editor = get_editor();
        let mut command = Command::new(editor.clone());
        if supports_line_num_arg(editor.as_str()) {
            let line_param = format!("+{}", line_number + 1);
            command.arg(line_param);
        }
        command.arg(self.filename);
        let status = command
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

fn init_term() -> Result<(), AppError> {
    let io_err = |detail: &str, e: std::io::Error| AppError::from_error("init_term", detail, e);
    terminal::enable_raw_mode().map_err(|e| io_err("enable_raw_mode", e))?;
    execute!(stdout(), Hide).map_err(|e| io_err("hide cursor", e))?;
    Ok(())
}

fn restore_term() {
    _ = execute!(stdout(), Show);
    _ = terminal::disable_raw_mode();
}

pub fn watch_and_report(filename: &str, dates: DateRange) -> Result<(), AppError> {
    init_term()?;
    defer! {
        restore_term();
    }
    let mut tracker = UI::new(filename);
    loop {
        let outcome = tracker.next_command()?;
        match outcome {
            UICommand::Quit => return Ok(()),
            UICommand::DoNothing => {}
            UICommand::Report(loaded) => {
                print_file(&loaded, dates)?;
            }
            UICommand::DisplayError(error) => {
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
    println!("\r\nPress r or ENTER to continue...\r");
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

fn get_editor() -> String {
    env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
}

fn supports_line_num_arg(editor: &str) -> bool {
    Regex::new(r"^(.*/)?((vim?)|(hx))$")
        .unwrap()
        .is_match(editor)
}
