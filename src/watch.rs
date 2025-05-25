use crate::menu::{Menu, MenuItem};
use crate::model::{Date, DateRange, DayEntry, Project};
use crate::report;
use crate::{append, parse};
use anyhow::{Context, Result, anyhow};
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
use std::time::{Duration, SystemTime};

enum RawReadResult {
    Char(char),
    Enter,
    Left,
    Right,
    Resized,
    Timeout,
}

#[derive(Copy, Clone)]
enum ReadResult {
    Append,
    Edit,
    Reload,
    Warnings,
    Quit,
    Left,
    Right,
    Resized,
    Timeout,
}

trait Terminal {
    fn start(&self) -> Result<()>;
    fn stop(&self) -> Result<()>;
    fn read(&self, timeout: Duration) -> Result<RawReadResult>;
    fn clear(&self) -> Result<()>;
    fn print_str(&self, s: &str) -> Result<()>;
    fn print_styled_str(&self, s: StyledContent<&str>) -> Result<()>;
    fn print_styled_string(&self, s: StyledContent<String>) -> Result<()>;
    fn goto(&self, row: u16, col: u16) -> Result<()>;
}

trait Storage {
    fn timestamp(&mut self, filename: &str) -> Result<u128>;
    fn load(&mut self, filename: &str) -> Result<LoadedFile>;
    fn append(
        &mut self,
        filename: &str,
        date: Date,
        recent_projects: Vector<&Project>,
    ) -> Result<()>;
}

trait AppLogic {
    fn run(
        &mut self,
        menu: &mut Menu<ReadResult>,
        terminal: &dyn Terminal,
        storage: &mut dyn Storage,
        editor: &mut dyn Editor,
    ) -> Result<UICommand>;
}

trait Editor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()>;
}

struct Writer {
    context: String,
    stdout: Stdout,
    result: Option<anyhow::Error>,
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
                .with_context(|| format!("{}: {}", self.context, error_message))
                .err();
        }
        self
    }

    fn write(&mut self) -> Result<()> {
        if self.result.is_none() {
            self.result = self
                .stdout
                .flush()
                .with_context(|| format!("{}: flush", self.context))
                .err();
        }
        self.result.take().map(Err).unwrap_or(Ok(()))
    }
}

struct RealTerminal {}
impl RealTerminal {
    fn print<T: Display>(&self, s: T) -> Result<()> {
        Writer::new("RealTerminal.println")
            .enqueue("Clear", terminal::Clear(terminal::ClearType::CurrentLine))
            .enqueue("Print", style::Print(s))
            .enqueue("MoveDown", cursor::MoveDown(1))
            .enqueue("MoveToColumn", cursor::MoveToColumn(0))
            .write()
    }
}

impl Terminal for RealTerminal {
    fn start(&self) -> Result<()> {
        let error_context = "init_term";
        terminal::enable_raw_mode()
            .with_context(|| format!("{}: enable_raw_mode", error_context))?;
        execute!(stdout(), Hide).with_context(|| format!("{}: hide cursor", error_context))?;
        Ok(())
    }

    fn stop(&self) -> Result<()> {
        _ = execute!(stdout(), Show);
        _ = terminal::disable_raw_mode();
        Ok(())
    }

    fn read(&self, timeout: Duration) -> Result<RawReadResult> {
        let error_context = "RealTerminal.read";
        while poll(timeout).with_context(|| format!("{}: poll", error_context))? {
            match read().with_context(|| format!("{}: read", error_context))? {
                Event::Key(event) => match event.code {
                    KeyCode::Char(c) => return Ok(RawReadResult::Char(c)),
                    KeyCode::Enter => return Ok(RawReadResult::Enter),
                    KeyCode::Left => return Ok(RawReadResult::Left),
                    KeyCode::Right => return Ok(RawReadResult::Right),
                    _ => {}
                },
                Event::Resize(_, _) => return Ok(RawReadResult::Resized),
                _ => {}
            }
        }
        Ok(RawReadResult::Timeout)
    }

    fn clear(&self) -> Result<()> {
        Writer::new("RealTerminal.clear")
            .enqueue("Clear", terminal::Clear(terminal::ClearType::All))
            .enqueue("MoveTo", cursor::MoveTo(0, 0))
            .write()
    }

    fn print_str(&self, s: &str) -> Result<()> {
        self.print(s)
    }

    fn print_styled_str(&self, s: StyledContent<&str>) -> Result<()> {
        self.print(s)
    }

    fn print_styled_string(&self, s: StyledContent<String>) -> Result<()> {
        self.print(s)
    }

    fn goto(&self, row: u16, col: u16) -> Result<()> {
        Writer::new("RealTerminal.goto")
            .enqueue("MoveTo", cursor::MoveTo(col, row))
            .write()
    }
}

struct RealStorage {}
impl Storage for RealStorage {
    fn timestamp(&mut self, filename: &str) -> Result<u128> {
        let error_context = "RealStorage.timestamp";
        let metadata =
            fs::metadata(filename).with_context(|| format!("{}: metadata", error_context))?;
        let modified = metadata
            .modified()
            .with_context(|| format!("{}: modified", error_context))?;
        let millis = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .with_context(|| format!("{}: duration_since", error_context))?
            .as_millis();
        Ok(millis)
    }

    fn load(&mut self, filename: &str) -> Result<LoadedFile> {
        let (day_entries, warnings) = parse::parse_file(filename)?;
        Ok(LoadedFile::new(&day_entries, &warnings))
    }

    fn append(
        &mut self,
        filename: &str,
        date: Date,
        recent_projects: Vector<&Project>,
    ) -> Result<()> {
        append::append_to_file(filename, date, recent_projects)
    }
}

struct RealEditor {}
impl Editor for RealEditor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()> {
        let error_context = "RealEditor.edit_file";
        let editor = get_editor();
        let mut command = Command::new(editor.clone());
        if supports_line_num_arg(editor.as_str()) {
            let line_param = format!("+{}", line_number + 1);
            command.arg(line_param);
        }
        command.arg(filename);
        let status = command
            .spawn()
            .with_context(|| format!("{}: spawn", error_context))?
            .wait()
            .with_context(|| format!("{}: wait", error_context))?;
        if !status.success() {
            return Err(anyhow!("{}: editor command failed", error_context));
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

    fn load(&mut self, storage: &mut dyn Storage, skip_delay: bool) -> Result<(bool, LoadedFile)> {
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

    fn append(&mut self, storage: &mut dyn Storage) -> Result<(bool, LoadedFile)> {
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
    ) -> Result<(bool, LoadedFile)> {
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

    fn read(&mut self, menu: &mut Menu<ReadResult>, terminal: &dyn Terminal) -> Result<ReadResult> {
        loop {
            match terminal.read(self.read_timeout)? {
                RawReadResult::Char(c) => match menu.select(c) {
                    Some(x) => return Ok(x),
                    None => continue,
                },
                RawReadResult::Enter => return Ok(menu.value()),
                RawReadResult::Left => return Ok(ReadResult::Left),
                RawReadResult::Right => return Ok(ReadResult::Right),
                RawReadResult::Timeout => return Ok(ReadResult::Timeout),
                RawReadResult::Resized => return Ok(ReadResult::Resized),
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
    ) -> Result<UICommand> {
        match self.read(menu, terminal)? {
            ReadResult::Quit => Ok(UICommand::Quit),
            ReadResult::Reload => match self.load(storage, true) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Warnings => Ok(UICommand::DisplayWarnings(self.loaded.clone())),
            ReadResult::Edit => match self.edit(storage, terminal, editor) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Append => match self.append(storage) {
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
    DisplayError(anyhow::Error),
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
) -> Result<()> {
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

pub fn watch_and_report(filename: &str, dates: &dyn Fn() -> DateRange) -> Result<()> {
    ui_impl(
        filename,
        dates,
        &mut create_menu()?,
        &RealTerminal {},
        &mut RealEditor {},
        &mut RealStorage {},
        &mut RealAppLogic::new(filename),
    )
}

fn create_menu() -> Result<Menu<ReadResult>> {
    let menu_items = vector!(
        MenuItem::new(ReadResult::Edit, "Edit", "Edit the file."),
        MenuItem::new(
            ReadResult::Append,
            "Append",
            "Add current date to the file."
        ),
        MenuItem::new(ReadResult::Reload, "Reload", "Reload file."),
        MenuItem::new(ReadResult::Warnings, "Warnings", "Display warnings."),
        MenuItem::new(ReadResult::Quit, "Quit", "Quit the program.")
    );
    Menu::new(menu_items)
}

fn display_menu(terminal: &dyn Terminal, menu: &Menu<ReadResult>) -> Result<()> {
    terminal.goto(0, 0)?;
    terminal.print_str(menu.render().as_str())?;
    terminal.print_styled_str(menu.description().yellow())
}

fn print_error(
    filename: &str,
    error: anyhow::Error,
    terminal: &dyn Terminal,
    menu: &Menu<ReadResult>,
) -> Result<()> {
    terminal.clear()?;
    display_menu(terminal, menu)?;
    terminal.goto(3, 0)?;
    terminal.print_styled_str("error:".red())?;
    terminal.print_styled_string(format!("   filename: {}", filename).red())?;
    terminal.print_styled_string(format!("    message: {:?}", error).red())
}

fn print_file(
    file: &LoadedFile,
    dates: DateRange,
    terminal: &dyn Terminal,
    menu: &Menu<ReadResult>,
) -> Result<()> {
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
) -> Result<()> {
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
