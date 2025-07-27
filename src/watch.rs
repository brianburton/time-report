use ratatui::{
    Terminal,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
};

use crate::model::{Date, DateRange, DayEntry, Project};
use crate::report;
use crate::report::ReportMode;
use crate::watch::WatchError::EditorExitCode;
use crate::watch::paragraph::ParagraphBuilder;
use crate::{append, parse};
use anyhow::Result;
use crossterm::event::{Event, poll, read};
use crossterm::event::{KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use derive_getters::Getters;
use im::{Vector, vector};
use menu::{Menu, MenuItem};
use mockall::automock;
use ratatui::buffer::Buffer;
use ratatui::prelude::{Backend, Rect, Widget};
use regex::Regex;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime, SystemTimeError, UNIX_EPOCH};
use thiserror::Error;

mod menu;
mod paragraph;

#[derive(Error, Debug)]
enum WatchError {
    #[error("Terminal read error: {0}")]
    TerminalRead(std::io::Error),
    #[error("Terminal write error: {0}")]
    TerminalWrite(std::io::Error),
    #[error("Failed to pause: {0}")]
    PauseFailed(std::io::Error),
    #[error("Failed to resume: {0}")]
    ResumeFailed(std::io::Error),
    #[error("Failed to read file timestamp: {0}")]
    TimestampReadFailed(std::io::Error),
    #[error("Failed to compute timestamp: {0}")]
    TimestampComputeFailed(SystemTimeError),
    #[error("Failed to run editor: {0}")]
    EditorFailure(std::io::Error),
    #[error("Failed to run editor: {0:?}")]
    EditorExitCode(Option<i32>),
}

pub fn watch_and_report(filename: &str, dates: &dyn Fn() -> DateRange) -> Result<()> {
    let menu = create_menu()?;
    let mut app_display = RealAppScreen {
        terminal: ratatui::init(),
    };
    let mut storage = RealStorage {};
    let mut editor = RealEditor {};
    let mut clock = RealClock {};
    let mut app_state = WatchApp::new(
        filename,
        dates,
        menu,
        &mut app_display,
        &mut storage,
        &mut editor,
        &mut clock,
    );
    let result = app_state.run();
    _ = app_display.terminal.clear();
    ratatui::restore();
    result
}

/// Subset of possible crossterm read() events supported by the application.
enum ScreenEvent {
    Char(char),
    Enter,
    Left,
    Right,
    Resized,
    Timeout,
    Scroll(ScrollAmount),
}

#[derive(Debug, Copy, Clone)]
enum ScrollAmount {
    DownLine,
    DownWeek,
    UpLine,
    UpWeek,
}

const SCROLL_UP_LINE: ScreenEvent = ScreenEvent::Scroll(ScrollAmount::UpLine);
const SCROLL_DOWN_LINE: ScreenEvent = ScreenEvent::Scroll(ScrollAmount::DownLine);
const SCROLL_UP_WEEK: ScreenEvent = ScreenEvent::Scroll(ScrollAmount::UpWeek);
const SCROLL_DOWN_WEEK: ScreenEvent = ScreenEvent::Scroll(ScrollAmount::DownWeek);

trait Renderable {
    fn render(&self, area: Rect, buf: &mut Buffer);
}

#[automock]
trait AppScreen {
    fn read(&self, timeout: Duration) -> Result<ScreenEvent>;
    fn draw(&mut self, screen: &dyn Renderable) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
}

#[automock]
trait Storage {
    fn timestamp(&mut self, filename: &str) -> Result<u128>;
    fn load(&mut self, dates: DateRange, filename: &str) -> Result<LoadedFile>;
    fn append(
        &mut self,
        filename: &str,
        date: Date,
        recent_projects: &Vector<Project>,
    ) -> Result<()>;
}

#[automock]
trait Editor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()>;
}

#[automock]
trait Clock {
    fn current_millis(&self) -> u128;
}

struct RealAppScreen<T: Backend> {
    terminal: Terminal<T>,
}

impl<T: Backend> AppScreen for RealAppScreen<T> {
    fn read(&self, timeout: Duration) -> Result<ScreenEvent> {
        while poll(timeout).map_err(WatchError::TerminalRead)? {
            match read().map_err(WatchError::TerminalRead)? {
                Event::Key(event) => match event.code {
                    KeyCode::Char('b') if event.modifiers == KeyModifiers::CONTROL => {
                        return Ok(SCROLL_UP_WEEK);
                    }
                    KeyCode::Char('f') if event.modifiers == KeyModifiers::CONTROL => {
                        return Ok(SCROLL_DOWN_WEEK);
                    }
                    KeyCode::Char(c) => return Ok(ScreenEvent::Char(c)),
                    KeyCode::Up => return Ok(SCROLL_UP_LINE),
                    KeyCode::Down => return Ok(SCROLL_DOWN_LINE),
                    KeyCode::PageUp => return Ok(SCROLL_UP_WEEK),
                    KeyCode::PageDown => return Ok(SCROLL_DOWN_WEEK),
                    KeyCode::Enter => return Ok(ScreenEvent::Enter),
                    KeyCode::Left => return Ok(ScreenEvent::Left),
                    KeyCode::Right => return Ok(ScreenEvent::Right),
                    _ => {}
                },
                Event::Resize(_, _) => return Ok(ScreenEvent::Resized),
                _ => {}
            }
        }
        Ok(ScreenEvent::Timeout)
    }

    fn draw(&mut self, screen: &dyn Renderable) -> Result<()> {
        self.terminal
            .draw(|frame| screen.render(frame.area(), frame.buffer_mut()))
            .map_err(WatchError::TerminalWrite)
            .map_err(anyhow::Error::from)
            .map(|_| ())
    }

    fn pause(&mut self) -> Result<()> {
        disable_raw_mode().map_err(WatchError::PauseFailed)?;
        self.terminal
            .clear()
            .map_err(WatchError::PauseFailed)
            .map_err(anyhow::Error::from)
    }

    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().map_err(WatchError::ResumeFailed)?;
        self.terminal
            .clear()
            .map_err(WatchError::ResumeFailed)
            .map_err(anyhow::Error::from)
    }
}

struct RealStorage {}

impl Storage for RealStorage {
    fn timestamp(&mut self, filename: &str) -> Result<u128> {
        let metadata = fs::metadata(filename).map_err(WatchError::TimestampReadFailed)?;
        let modified = metadata
            .modified()
            .map_err(WatchError::TimestampReadFailed)?;
        let millis = modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(WatchError::TimestampComputeFailed)?
            .as_millis();
        Ok(millis)
    }

    fn load(&mut self, dates: DateRange, filename: &str) -> Result<LoadedFile> {
        let current_file_millis = self.timestamp(filename)?;
        let (day_entries, warnings) = parse::parse_file(filename)?;
        let min_date = dates.first().minus_days(30)?;
        let recent_projects = append::recent_projects(&day_entries, min_date, 5);
        let day_entries = report::day_entries_in_range(&dates, &day_entries);
        Ok(LoadedFile::new(
            dates,
            &day_entries,
            &warnings,
            &recent_projects,
            current_file_millis,
        ))
    }

    fn append(
        &mut self,
        filename: &str,
        date: Date,
        recent_projects: &Vector<Project>,
    ) -> Result<()> {
        append::append_to_file(filename, date, recent_projects)
    }
}

struct RealEditor {}
impl Editor for RealEditor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()> {
        let editor = get_editor();
        let mut command = Command::new(editor.clone());
        if supports_line_num_arg(editor.as_str()) {
            let line_param = format!("+{}", line_number + 1);
            command.arg(line_param);
        }
        command.arg(filename);
        let status = command
            .spawn()
            .map_err(WatchError::EditorFailure)?
            .wait()
            .map_err(WatchError::EditorFailure)?;
        if !status.success() {
            return Err(EditorExitCode(status.code()).into());
        }
        Ok(())
    }
}

struct RealClock {}
impl Clock for RealClock {
    fn current_millis(&self) -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unable to read current time")
            .as_millis()
    }
}

#[derive(Clone, Getters)]
struct LoadedFile {
    dates: DateRange,
    day_entries: Vector<DayEntry>,
    warnings: Vector<String>,
    recent_projects: Vector<Project>,
    load_time_millis: u128,
}

impl LoadedFile {
    fn new(
        dates: DateRange,
        day_entries: &Vector<DayEntry>,
        warnings: &Vector<String>,
        recent_projects: &Vector<Project>,
        load_time_millis: u128,
    ) -> Self {
        LoadedFile {
            dates,
            day_entries: day_entries.clone(),
            warnings: warnings.clone(),
            recent_projects: recent_projects.clone(),
            load_time_millis,
        }
    }

    fn empty(dates: DateRange) -> Self {
        LoadedFile {
            dates,
            day_entries: Vector::new(),
            warnings: Vector::new(),
            recent_projects: Vector::new(),
            load_time_millis: 0,
        }
    }
}

#[derive(Debug, Copy, Clone)]
enum UserRequest {
    Append,
    Edit,
    Reload,
    Warnings,
    Quit,
    Left,
    Right,
    Resized,
    Timeout,
    ToggleReportMode,
    Scroll(ScrollAmount),
}

enum DisplayContent {
    Report(LoadedFile),
    Warnings(LoadedFile),
    Error(anyhow::Error),
}

enum UICommand {
    DoNothing,
    Quit,
    Report(LoadedFile),
    UpdateMenu,
    DisplayWarnings(LoadedFile),
    DisplayError(anyhow::Error),
}

struct WatchApp<'a, TAppScreen, TStorage, TEditor, TClock>
where
    TAppScreen: AppScreen,
    TStorage: Storage,
    TEditor: Editor,
    TClock: Clock,
{
    loaded: LoadedFile,
    menu: Menu<UserRequest>,
    filename: &'a str,
    read_timeout: Duration,
    update_delay_millis: u128,
    report_mode: ReportMode,
    start_line: usize,
    line_count: usize,
    section_starts: Vector<usize>,
    dates: &'a dyn Fn() -> DateRange,
    app_screen: &'a mut TAppScreen,
    storage: &'a mut TStorage,
    editor: &'a mut TEditor,
    clock: &'a mut TClock,
}

fn first_after(offsets: &Vector<usize>, target: usize) -> usize {
    offsets
        .iter()
        .find(|o| **o > target)
        .map(|o| *o)
        .unwrap_or(target)
}

fn last_before(offsets: &Vector<usize>, target: usize) -> usize {
    offsets
        .iter()
        .rfind(|o| **o < target)
        .map(|o| *o)
        .unwrap_or(target)
}

impl<'a, TAppScreen, TStorage, TEditor, TClock> WatchApp<'a, TAppScreen, TStorage, TEditor, TClock>
where
    TAppScreen: AppScreen,
    TStorage: Storage,
    TEditor: Editor,
    TClock: Clock,
{
    fn new(
        filename: &'a str,
        dates: &'a dyn Fn() -> DateRange,
        menu: Menu<UserRequest>,
        app_screen: &'a mut TAppScreen,
        storage: &'a mut TStorage,
        editor: &'a mut TEditor,
        clock: &'a mut TClock,
    ) -> WatchApp<'a, TAppScreen, TStorage, TEditor, TClock> {
        WatchApp {
            filename,
            dates,
            loaded: LoadedFile::empty(dates()),
            update_delay_millis: 500,
            read_timeout: Duration::from_millis(100),
            report_mode: ReportMode::Detail,
            start_line: 0,
            line_count: 0,
            section_starts: Vector::new(),
            menu,
            app_screen,
            storage,
            editor,
            clock,
        }
    }

    fn run(&mut self) -> Result<()> {
        let mut on_screen = DisplayContent::Report(self.loaded.clone());
        loop {
            let user_request = self.read_user_request()?;
            let ui_command = self.process_user_request(user_request)?;
            match ui_command {
                UICommand::Quit => return Ok(()),
                UICommand::DoNothing => continue,
                UICommand::UpdateMenu => (),
                UICommand::Report(loaded) => on_screen = DisplayContent::Report(loaded),
                UICommand::DisplayWarnings(loaded) => on_screen = DisplayContent::Warnings(loaded),
                UICommand::DisplayError(error) => on_screen = DisplayContent::Error(error),
            };
            self.update_screen(&on_screen)?;
        }
    }

    fn scroll(&mut self, amount: ScrollAmount) -> Result<UICommand> {
        match amount {
            ScrollAmount::DownLine if self.start_line < self.line_count - 1 => self.start_line += 1,
            ScrollAmount::UpLine if self.start_line > 0 => self.start_line -= 1,
            ScrollAmount::DownWeek => {
                self.start_line = first_after(&self.section_starts, self.start_line);
            }
            ScrollAmount::UpWeek => {
                self.start_line = last_before(&self.section_starts, self.start_line)
            }
            _ => (),
        }
        Ok(UICommand::Report(self.loaded.clone()))
    }

    fn update_screen(&mut self, what_to_display: &DisplayContent) -> Result<()> {
        match what_to_display {
            DisplayContent::Report(loaded_file) => {
                let report =
                    ReportScreen::new(&self.menu, loaded_file, self.report_mode, self.start_line);
                match report {
                    Ok(report) => {
                        self.line_count = report.report.line_count();
                        self.section_starts = report.report.section_starts();
                        self.app_screen.draw(&report)
                    }
                    Err(error) => {
                        self.app_screen
                            .draw(&ErrorScreen::new(&self.menu, self.filename, &error))
                    }
                }
            }
            DisplayContent::Warnings(loaded_file) => self
                .app_screen
                .draw(&WarningsScreen::new(&self.menu, loaded_file)),
            DisplayContent::Error(error) => {
                self.app_screen
                    .draw(&ErrorScreen::new(&self.menu, self.filename, error))
            }
        }
    }

    fn process_user_request(&mut self, event: UserRequest) -> Result<UICommand> {
        let result = match event {
            UserRequest::Quit => Ok(UICommand::Quit),
            UserRequest::Reload => self.load(true),
            UserRequest::Warnings => Ok(UICommand::DisplayWarnings(self.loaded.clone())),
            UserRequest::Edit => self.edit(),
            UserRequest::Append => self.append(),
            UserRequest::Left => self.change_menu_selection(event),
            UserRequest::Right => self.change_menu_selection(event),
            UserRequest::Resized => Ok(UICommand::Report(self.loaded.clone())),
            UserRequest::Timeout => self.load(false),
            UserRequest::ToggleReportMode => self.toggle_report_mode(),
            UserRequest::Scroll(amount) => self.scroll(amount),
        };
        result.or_else(|e| Ok(UICommand::DisplayError(e)))
    }

    fn change_menu_selection(&mut self, user_request: UserRequest) -> Result<UICommand> {
        match user_request {
            UserRequest::Left => self.menu = self.menu.left(),
            UserRequest::Right => self.menu = self.menu.right(),
            _ => (),
        };
        Ok(UICommand::UpdateMenu)
    }

    fn read_user_request(&mut self) -> Result<UserRequest> {
        loop {
            match self.app_screen.read(self.read_timeout)? {
                ScreenEvent::Char(c) => match self.menu.select(c) {
                    Some(x) => {
                        self.menu = x;
                        return Ok(self.menu.value());
                    }
                    None => continue,
                },
                ScreenEvent::Enter => return Ok(self.menu.value()),
                ScreenEvent::Left => return Ok(UserRequest::Left),
                ScreenEvent::Right => return Ok(UserRequest::Right),
                ScreenEvent::Timeout => return Ok(UserRequest::Timeout),
                ScreenEvent::Resized => return Ok(UserRequest::Resized),
                ScreenEvent::Scroll(amount) => return Ok(UserRequest::Scroll(amount)),
            }
        }
    }

    fn load(&mut self, force_reload: bool) -> Result<UICommand> {
        let dates = self.date_range();
        if !self.load_needed(dates, force_reload)? {
            return Ok(UICommand::DoNothing);
        }

        self.loaded = self.storage.load(dates, self.filename)?;
        if self.loaded.day_entries.is_empty() {
            self.loaded
                .warnings
                .push_front(format!("No day entries found in date range: {dates}."));
        }

        Ok(UICommand::Report(self.loaded.clone()))
    }

    fn load_needed(&mut self, dates: DateRange, force_reload: bool) -> Result<bool> {
        if force_reload || self.loaded.dates != dates {
            return Ok(true);
        }

        let current_file_millis = self.storage.timestamp(self.filename)?;
        if current_file_millis == self.loaded.load_time_millis {
            return Ok(false);
        }

        let current_time_millis = self.clock.current_millis();
        let next_update_millis = current_time_millis - self.update_delay_millis;
        Ok(current_file_millis < next_update_millis)
    }

    fn date_range(&self) -> DateRange {
        (self.dates)()
    }

    fn append(&mut self) -> Result<UICommand> {
        self.load(true)?;
        let date = Date::today();
        let day_entries = self.loaded.day_entries();
        append::validate_date(day_entries, date)?;

        self.storage
            .append(self.filename, date, &self.loaded.recent_projects)?;
        self.load(true)
    }

    fn edit(&mut self) -> Result<UICommand> {
        let line_number = *self
            .find_today_or_later(Date::today())
            .map(|date| date.line_number())
            .unwrap_or(&0);

        self.app_screen.pause()?;
        let rc = self
            .editor
            .edit_file(self.filename, line_number)
            .and_then(|_| self.load(true));
        _ = self.app_screen.resume();
        rc
    }

    fn find_today_or_later(&self, today: Date) -> Option<&DayEntry> {
        self.loaded
            .day_entries()
            .iter()
            .find(|e| e.date() >= &today)
            .or_else(|| self.loaded.day_entries().last())
    }

    fn toggle_report_mode(&mut self) -> Result<UICommand> {
        self.report_mode = self.report_mode.toggle();
        Ok(UICommand::Report(self.loaded.clone()))
    }
}

fn create_menu() -> Result<Menu<UserRequest>> {
    let menu_items = vector!(
        MenuItem::new(
            UserRequest::Edit,
            "Edit",
            format!("Edit the file using {} and reload.", get_editor_name()).as_str(),
            'e'
        ),
        MenuItem::new(
            UserRequest::Append,
            "Append",
            "Add current date to the file along with some recently used projects.",
            'a'
        ),
        MenuItem::new(
            UserRequest::ToggleReportMode,
            "Mode",
            "Toggle between detailed and summary report mode.",
            'm'
        ),
        MenuItem::new(
            UserRequest::Reload,
            "Reload",
            "Reload file and display report.",
            'r'
        ),
        MenuItem::new(
            UserRequest::Warnings,
            "Warnings",
            "Display all warnings.",
            'w'
        ),
        MenuItem::new(UserRequest::Quit, "Quit", "Quit the program.", 'q')
    );
    Menu::new(menu_items)
}

fn menu_style(selected: bool) -> Style {
    if selected {
        Style::new()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
    } else {
        Style::new().fg(Color::Blue)
    }
}

fn format_menu<T: Copy>(menu: &Menu<T>) -> ParagraphBuilder {
    let mut builder = ParagraphBuilder::new();
    builder.add_plain(" ".to_string());
    for (index, item) in menu.items().iter().enumerate() {
        let selected = index == *menu.selected_index();
        let style = menu_style(selected);
        builder
            .add_styled(item.display().to_string(), style)
            .add_plain("  ".to_string());
    }

    builder
        .new_line()
        .add_plain(" ".to_string())
        .add_plain(menu.description().to_string())
        .new_line()
        .bordered();
    builder
}

const MENU_HEIGHT: u16 = 4;
const WARNING_HEIGHT: u16 = 3;

fn format_warnings_summary(file: &LoadedFile) -> ParagraphBuilder {
    let style = Style::new().fg(Color::Red);
    let text = match file.warnings.len() {
        0 => "".to_string(),
        1 => file.warnings.get(0).unwrap().clone(),
        _ => format!(" There are {} warnings.", file.warnings.len()),
    };
    let mut builder = ParagraphBuilder::new();
    builder
        .add_styled(text, style)
        .new_line()
        .titled(" Warnings ".to_string());
    builder
}

fn format_warnings(file: &LoadedFile) -> ParagraphBuilder {
    let mut builder = ParagraphBuilder::new();
    if file.warnings.is_empty() {
        builder
            .add_plain(" There are no warnings to display.".to_string())
            .new_line();
    } else {
        let style = Style::new().fg(Color::Red);
        for warning in file.warnings.iter() {
            builder
                .add_styled(format!(" warning: {warning}"), style)
                .new_line();
        }
    }
    builder.bordered();
    builder
}

fn format_report(
    file: &LoadedFile,
    report_mode: ReportMode,
    start_line: usize,
) -> Result<ParagraphBuilder> {
    let mut builder = ParagraphBuilder::new();
    for line in report::create_report(file.dates, &file.day_entries, report_mode)? {
        builder
            .add_plain(" ".to_string())
            .add_plain(line)
            .new_line();
    }
    let title = match report_mode {
        ReportMode::Detail => " Detail Report ",
        ReportMode::Summary => " Summary Report ",
    };
    builder.titled(title.to_string()).start_line(start_line);
    Ok(builder)
}

fn format_error(filename: &str, error: &anyhow::Error) -> ParagraphBuilder {
    let style = Style::new().fg(Color::Red);
    let lines = format!("{error:?}")
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut builder = ParagraphBuilder::new();
    builder
        .add_styled(format!("   filename: {filename}"), style)
        .new_line()
        .add_styled(
            format!(
                "    message: {}",
                lines.first().map(|s| s.as_ref()).unwrap_or("")
            ),
            style,
        )
        .new_line();
    for line in lines.iter().skip(1) {
        builder
            .add_styled(format!("           : {line}"), style)
            .new_line();
    }
    builder
}

struct ReportScreen {
    menu: ParagraphBuilder,
    report: ParagraphBuilder,
    warnings: ParagraphBuilder,
}

impl ReportScreen {
    fn new(
        menu: &Menu<UserRequest>,
        file: &LoadedFile,
        report_mode: ReportMode,
        start_line: usize,
    ) -> Result<Self> {
        let screen = ReportScreen {
            menu: format_menu(menu),
            report: format_report(file, report_mode, start_line)?,
            warnings: format_warnings_summary(file),
        };
        Ok(screen)
    }
}

impl Renderable for ReportScreen {
    fn render(&self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0), Length(WARNING_HEIGHT)]);
        let [menu_area, report_area, warnings_area] = vertical.areas(area);
        self.menu.build().render(menu_area, buf);
        self.report.build().render(report_area, buf);
        self.warnings.build().render(warnings_area, buf);
    }
}

struct WarningsScreen {
    menu: ParagraphBuilder,
    warnings: ParagraphBuilder,
}

impl WarningsScreen {
    fn new(menu: &Menu<UserRequest>, file: &LoadedFile) -> Self {
        WarningsScreen {
            menu: format_menu(menu),
            warnings: format_warnings(file),
        }
    }
}

impl Renderable for WarningsScreen {
    fn render(&self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0)]);
        let [menu_area, warnings_area] = vertical.areas(area);
        self.menu.build().render(menu_area, buf);
        self.warnings.build().render(warnings_area, buf);
    }
}

struct ErrorScreen {
    menu: ParagraphBuilder,
    error: ParagraphBuilder,
}

impl ErrorScreen {
    fn new(menu: &Menu<UserRequest>, filename: &str, error: &anyhow::Error) -> Self {
        ErrorScreen {
            menu: format_menu(menu),
            error: format_error(filename, error),
        }
    }
}

impl Renderable for ErrorScreen {
    fn render(&self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        use Constraint::{Length, Min};
        let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0)]);
        let [menu_area, error_area] = vertical.areas(area);
        self.menu.build().render(menu_area, buf);
        self.error.build().render(error_area, buf);
    }
}

fn get_editor() -> String {
    env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
}

fn get_editor_name() -> String {
    let editor_command = get_editor();
    let regex: Regex = Regex::new(r"^([^ ]*/)?([^ /]+)").unwrap();
    let editor = regex.captures_iter(editor_command.as_str()).last();
    editor.map(|c| c[2].to_string()).unwrap_or(editor_command)
}

fn supports_line_num_arg(editor: &str) -> bool {
    Regex::new(r"^(.*/)?((vim?)|(hx))$")
        .unwrap()
        .is_match(editor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use filetime::{FileTime, set_file_times};
    use tempfile::NamedTempFile;

    #[test]
    fn test_mock_clock() {
        let mut c = MockClock::new();
        c.expect_current_millis().return_const(1000u128);
        assert_eq!(c.current_millis(), 1000u128);
    }

    #[test]
    fn test_get_editor_name() {
        let key = "EDITOR";
        unsafe {
            env::set_var(key, "vi");
            assert_eq!(get_editor_name(), "vi".to_string());

            env::set_var(key, "/usr/bin/vi");
            assert_eq!(get_editor_name(), "vi".to_string());

            env::set_var(key, "/usr/bin/emacsclient -n");
            assert_eq!(get_editor_name(), "emacsclient".to_string());
        }
    }

    #[test]
    fn test_timestamp() {
        let datetime_str = "2024-06-09T12:34:55Z";
        let dt: DateTime<Utc> = datetime_str.parse().unwrap(); // ISO 8601 format
        let system_time: SystemTime = SystemTime::from(dt);

        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path();

        // Set the access and modification times
        let new_atime = FileTime::from_system_time(system_time);
        set_file_times(path, new_atime, new_atime).unwrap();

        let mut storage = RealStorage {};
        assert_eq!(
            storage.timestamp(path.to_str().unwrap()).unwrap(),
            1717936495000
        );
    }

    #[test]
    fn test_offsets() {
        let start_offsets: Vector<usize> = vector!(0, 7, 12);
        assert_eq!(first_after(&start_offsets, 0), 7);
        assert_eq!(first_after(&start_offsets, 6), 7);
        assert_eq!(first_after(&start_offsets, 7), 12);
        assert_eq!(first_after(&start_offsets, 12), 12);
        assert_eq!(first_after(&start_offsets, 13), 13);
        assert_eq!(last_before(&start_offsets, 0), 0);
        assert_eq!(last_before(&start_offsets, 1), 0);
        assert_eq!(last_before(&start_offsets, 7), 0);
        assert_eq!(last_before(&start_offsets, 8), 7);
        assert_eq!(last_before(&start_offsets, 12), 7);
        assert_eq!(last_before(&start_offsets, 13), 12);
    }
}
