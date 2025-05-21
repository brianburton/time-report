use crate::core::AppError;
use crate::menu::{Menu, MenuItem};
use crate::model::{Date, DateRange, DayEntry, Project};
use crate::report;
use crate::{append, parse};
use crossterm::cursor::{Hide, Show};
use crossterm::event::KeyCode;
use crossterm::event::{Event, poll, read};
use crossterm::style::{StyledContent, Stylize};
use crossterm::{QueueableCommand, cursor, execute, style, terminal};
use derive_getters::Getters;
use im::{Vector, vector};
use regex::Regex;
use scopeguard::defer;
use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{Stdout, Write, stdout};
use std::process::Command;
use std::time::{Duration, SystemTime, SystemTimeError};

#[derive(Copy, Clone)]
enum ReadResult {
    Char(char),
    Enter,
    Left,
    Right,
    Resized,
    Timeout,
}

trait Terminal {
    fn start(&self) -> Result<(), AppError>;
    fn stop(&self) -> Result<(), AppError>;
    fn read(&self, timeout: Duration) -> Result<ReadResult, AppError>;
    fn clear(&self) -> Result<(), AppError>;
    fn print_str(&self, s: &str) -> Result<(), AppError>;
    fn print_styled_str(&self, s: StyledContent<&str>) -> Result<(), AppError>;
    fn print_styled_string(&self, s: StyledContent<String>) -> Result<(), AppError>;
    fn goto(&self, row: u16, col: u16) -> Result<(), AppError>;
}

trait Storage {
    fn timestamp(&mut self, filename: &str) -> Result<u128, AppError>;
    fn load(&mut self, filename: &str) -> Result<LoadedFile, AppError>;
    fn append(
        &mut self,
        filename: &str,
        date: Date,
        recent_projects: Vector<&Project>,
    ) -> Result<(), AppError>;
}

trait AppLogic {
    fn run(
        &mut self,
        menu: &mut Menu<ReadResult>,
        terminal: &dyn Terminal,
        storage: &mut dyn Storage,
        editor: &mut dyn Editor,
    ) -> Result<UICommand, AppError>;
}

trait Editor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<(), AppError>;
}

struct Writer {
    context: String,
    stdout: Stdout,
    result: Option<AppError>,
}

impl Writer {
    fn new(context: &str) -> Self {
        Writer {
            context: context.to_string(),
            stdout: stdout(),
            result: None,
        }
    }

    fn enqueue<T: crossterm::Command>(&mut self, error_message: &str, command: T) -> &mut Self {
        if self.result.is_none() {
            self.result = self
                .stdout
                .queue(command)
                .err()
                .map(|e| AppError::from_error(self.context.as_str(), error_message, e));
        }
        self
    }

    fn write(&mut self) -> Result<(), AppError> {
        if self.result.is_none() {
            self.result = self
                .stdout
                .flush()
                .err()
                .map(|e| AppError::from_error(self.context.as_str(), "flush", e));
        }
        self.result.take().map(Err).unwrap_or(Ok(()))
    }
}

struct RealTerminal {}
impl RealTerminal {
    fn print<T: Display>(&self, s: style::Print<T>) -> Result<(), AppError> {
        Writer::new("RealTerminal.println")
            .enqueue("Clear", terminal::Clear(terminal::ClearType::CurrentLine))
            .enqueue("Print", style::Print(s))
            .enqueue("MoveDown", cursor::MoveDown(1))
            .enqueue("MoveToColumn", cursor::MoveToColumn(0))
            .write()
    }
}

impl Terminal for RealTerminal {
    fn start(&self) -> Result<(), AppError> {
        let io_err = |detail: &str, e: std::io::Error| AppError::from_error("init_term", detail, e);
        terminal::enable_raw_mode().map_err(|e| io_err("enable_raw_mode", e))?;
        execute!(stdout(), Hide).map_err(|e| io_err("hide cursor", e))?;
        Ok(())
    }

    fn stop(&self) -> Result<(), AppError> {
        _ = execute!(stdout(), Show);
        _ = terminal::disable_raw_mode();
        Ok(())
    }

    fn read(&self, timeout: Duration) -> Result<ReadResult, AppError> {
        let io_err =
            |detail: &str, e: std::io::Error| AppError::from_error("RealTerminal.read", detail, e);
        while poll(timeout).map_err(|e| io_err("poll", e))? {
            match read().map_err(|e| io_err("read", e))? {
                Event::Key(event) => match event.code {
                    KeyCode::Char(c) => return Ok(ReadResult::Char(c)),
                    KeyCode::Enter => return Ok(ReadResult::Enter),
                    KeyCode::Left => return Ok(ReadResult::Left),
                    KeyCode::Right => return Ok(ReadResult::Right),
                    _ => {}
                },
                Event::Resize(_, _) => return Ok(ReadResult::Resized),
                _ => {}
            }
        }
        Ok(ReadResult::Timeout)
    }

    fn clear(&self) -> Result<(), AppError> {
        Writer::new("RealTerminal.clear")
            .enqueue("Clear", terminal::Clear(terminal::ClearType::All))
            .enqueue("MoveTo", cursor::MoveTo(0, 0))
            .write()
    }

    fn print_str(&self, s: &str) -> Result<(), AppError> {
        self.print(style::Print(s))
    }

    fn print_styled_str(&self, s: StyledContent<&str>) -> Result<(), AppError> {
        self.print(style::Print(s))
    }

    fn print_styled_string(&self, s: StyledContent<String>) -> Result<(), AppError> {
        self.print(style::Print(s))
    }

    fn goto(&self, row: u16, col: u16) -> Result<(), AppError> {
        Writer::new("RealTerminal.goto")
            .enqueue("MoveTo", cursor::MoveTo(col, row))
            .write()
    }
}

struct RealStorage {}
impl Storage for RealStorage {
    fn timestamp(&mut self, filename: &str) -> Result<u128, AppError> {
        let io_err = |detail: &str, e: std::io::Error| {
            AppError::from_error("RealStorage.timestamp", detail, e)
        };
        let time_err = |detail: &str, e: SystemTimeError| {
            AppError::from_error("RealStorage.timestamp", detail, e)
        };
        let metadata = fs::metadata(filename).map_err(|e| io_err("metadata", e))?;
        let modified = metadata.modified().map_err(|e| io_err("modified", e))?;
        let millis = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| time_err("duration_since", e))?
            .as_millis();
        Ok(millis)
    }

    fn load(&mut self, filename: &str) -> Result<LoadedFile, AppError> {
        let (day_entries, warnings) = parse::parse_file(filename)?;
        Ok(LoadedFile::new(&day_entries, &warnings))
    }

    fn append(
        &mut self,
        filename: &str,
        date: Date,
        recent_projects: Vector<&Project>,
    ) -> Result<(), AppError> {
        append::append_to_file(filename, date, recent_projects)
    }
}

struct RealEditor {}
impl Editor for RealEditor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<(), AppError> {
        let io_err = |detail: &str, e: std::io::Error| {
            AppError::from_error("RealEditor.edit_file", detail, e)
        };
        let editor = get_editor();
        let mut command = Command::new(editor.clone());
        if supports_line_num_arg(editor.as_str()) {
            let line_param = format!("+{}", line_number + 1);
            command.arg(line_param);
        }
        command.arg(filename);
        let status = command
            .spawn()
            .map_err(|e| io_err("spawn", e))?
            .wait()
            .map_err(|e| io_err("wait", e))?;
        if !status.success() {
            return Err(AppError::from_str("edit", "editor command failed"));
        }
        Ok(())
    }
}

struct RealAppLogic<'a> {
    filename: &'a str,
    last_update_millis: u128,
    loaded: LoadedFile,
    read_timeout: Duration,
    update_delay_millis: u128,
}

impl<'a> RealAppLogic<'a> {
    fn new(filename: &'a str) -> RealAppLogic<'a> {
        RealAppLogic {
            filename,
            loaded: LoadedFile::empty(),
            last_update_millis: 0,
            update_delay_millis: 500,
            read_timeout: Duration::from_millis(100),
        }
    }

    fn load(
        &mut self,
        storage: &mut dyn Storage,
        skip_delay: bool,
    ) -> Result<(bool, LoadedFile), AppError> {
        let current_file_millis = storage.timestamp(self.filename)?;
        if current_file_millis == self.last_update_millis {
            return Ok((false, self.loaded.clone()));
        }
        let next_update_millis = self.last_update_millis + self.update_delay_millis;
        if current_file_millis < next_update_millis && !skip_delay {
            return Ok((false, self.loaded.clone()));
        }
        self.last_update_millis = current_file_millis;
        self.loaded = storage.load(self.filename)?;
        Ok((true, self.loaded.clone()))
    }

    fn append(&mut self, storage: &mut dyn Storage) -> Result<(bool, LoadedFile), AppError> {
        self.load(storage, true)?;
        let date = Date::today();
        let day_entries = self.loaded.day_entries();
        append::validate_date(day_entries, date)?;

        let min_date = date.minus_days(30)?;
        let recent_projects = append::recent_projects(day_entries, min_date, 5);
        storage.append(self.filename, date, recent_projects)?;
        self.load(storage, true)
    }

    fn edit(
        &mut self,
        storage: &mut dyn Storage,
        terminal: &dyn Terminal,
        editor: &mut dyn Editor,
    ) -> Result<(bool, LoadedFile), AppError> {
        let line_number = self
            .loaded
            .day_entries()
            .last()
            .map(|e| e.line_number())
            .unwrap_or(&0);

        terminal.stop()?;
        defer! {
            _=terminal.start();
        }

        editor.edit_file(self.filename, *line_number)?;
        self.load(storage, true)
    }

    fn read(
        &mut self,
        menu: &mut Menu<ReadResult>,
        terminal: &dyn Terminal,
    ) -> Result<ReadResult, AppError> {
        loop {
            match terminal.read(self.read_timeout)? {
                ReadResult::Char(c) => {
                    if let Some(x) = menu.select(c) {
                        return Ok(x);
                    } else {
                        continue;
                    }
                }
                ReadResult::Enter => return Ok(ReadResult::Char(menu.key())),
                rr => {
                    return Ok(rr);
                }
            }
        }
    }
}

impl AppLogic for RealAppLogic<'_> {
    fn run(
        &mut self,
        menu: &mut Menu<ReadResult>,
        terminal: &dyn Terminal,
        storage: &mut dyn Storage,
        editor: &mut dyn Editor,
    ) -> Result<UICommand, AppError> {
        match self.read(menu, terminal)? {
            ReadResult::Char('q') => Ok(UICommand::Quit),
            ReadResult::Char('r') => match self.load(storage, true) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Char('w') => Ok(UICommand::DisplayWarnings(self.loaded.clone())),
            ReadResult::Char('e') => match self.edit(storage, terminal, editor) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Char('a') => match self.append(storage) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Left => {
                menu.left();
                Ok(UICommand::UpdateMenu)
            }
            ReadResult::Right => {
                menu.right();
                Ok(UICommand::UpdateMenu)
            }
            ReadResult::Resized => Ok(UICommand::Report(self.loaded.clone())),
            ReadResult::Timeout => match self.load(storage, false) {
                Ok((true, loaded)) => Ok(UICommand::Report(loaded)),
                Ok((false, _)) => Ok(UICommand::DoNothing),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            _ => Ok(UICommand::DoNothing),
        }
    }
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
    UpdateMenu,
    DisplayWarnings(LoadedFile),
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

fn ui_impl(
    filename: &str,
    dates: &dyn Fn() -> DateRange,
    menu: &mut Menu<ReadResult>,
    terminal: &dyn Terminal,
    editor: &mut dyn Editor,
    storage: &mut dyn Storage,
    logic: &mut dyn AppLogic,
) -> Result<(), AppError> {
    terminal.start()?;
    defer! {
        _=terminal.stop();
    }

    loop {
        let outcome = logic.run(menu, terminal, storage, editor)?;
        match outcome {
            UICommand::Quit => {
                terminal.clear()?;
                return Ok(());
            }
            UICommand::DoNothing => {}
            UICommand::Report(loaded) => {
                print_file(&loaded, dates(), terminal, menu)?;
            }
            UICommand::UpdateMenu => {
                display_menu(terminal, menu)?;
            }
            UICommand::DisplayWarnings(loaded) => {
                print_warnings(&loaded, terminal, menu)?;
            }
            UICommand::DisplayError(error) => {
                print_error(filename, error, terminal, menu)?;
            }
        }
    }
}

pub fn watch_and_report(filename: &str, dates: &dyn Fn() -> DateRange) -> Result<(), AppError> {
    let mut menu = create_menu();
    ui_impl(
        filename,
        dates,
        &mut menu,
        &RealTerminal {},
        &mut RealEditor {},
        &mut RealStorage {},
        &mut RealAppLogic::new(filename),
    )
}

fn create_menu() -> Menu<ReadResult> {
    let menu_items = vector!(
        MenuItem::new(ReadResult::Char('e'), "Edit", "Edit the file."),
        MenuItem::new(
            ReadResult::Char('a'),
            "Append",
            "Add current date to the file."
        ),
        MenuItem::new(ReadResult::Char('r'), "Reload", "Reload file."),
        MenuItem::new(ReadResult::Char('w'), "Warnings", "Display warnings."),
        MenuItem::new(ReadResult::Char('q'), "Quit", "Quit the program.")
    );
    Menu::new(menu_items.clone())
}

fn display_menu(terminal: &dyn Terminal, menu: &Menu<ReadResult>) -> Result<(), AppError> {
    terminal.goto(0, 0)?;
    terminal.print_str(menu.render().as_str())?;
    terminal.print_styled_str(menu.description().yellow())
}

fn print_error(
    filename: &str,
    error: AppError,
    terminal: &dyn Terminal,
    menu: &Menu<ReadResult>,
) -> Result<(), AppError> {
    terminal.clear()?;
    display_menu(terminal, menu)?;
    terminal.goto(3, 0)?;
    terminal.print_styled_str("error:".red())?;
    terminal.print_styled_string(format!("   filename: {}", filename).red())?;
    terminal.print_styled_string(format!("    context: {}", error.context()).red())?;
    terminal.print_styled_string(format!("     detail: {}", error.detail()).red())
}

fn print_file(
    file: &LoadedFile,
    dates: DateRange,
    terminal: &dyn Terminal,
    menu: &Menu<ReadResult>,
) -> Result<(), AppError> {
    terminal.clear()?;
    display_menu(terminal, menu)?;
    terminal.goto(3, 0)?;
    match file.warnings.len() {
        0 => (),
        1 => {
            terminal
                .print_styled_string(format!("warning: {}", file.warnings[0].as_str()).red())?;
            terminal.print_str("")?;
        }
        _ => {
            terminal.print_styled_string(
                format!("There are {} warnings.", file.warnings.len()).red(),
            )?;
            terminal.print_str("")?;
        }
    }
    let lines = report::create_report(dates, &file.day_entries)?;
    for line in lines {
        terminal.print_str(line.as_str())?;
    }
    Ok(())
}

fn print_warnings(
    file: &LoadedFile,
    terminal: &dyn Terminal,
    menu: &Menu<ReadResult>,
) -> Result<(), AppError> {
    terminal.clear()?;
    display_menu(terminal, menu)?;
    terminal.goto(3, 0)?;
    if file.warnings.is_empty() {
        terminal.print_str("There are no warnings to display.")?;
    } else {
        for warning in &file.warnings {
            terminal.print_styled_string(format!("warning: {}", warning.as_str()).red())?;
        }
    }
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
