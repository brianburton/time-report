use ratatui::{
    Terminal,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
};

use crate::model::{Date, DateRange, DayEntry, Project};
use crate::report;
use crate::watch::paragraph::ParagraphBuilder;
use crate::{append, parse};
use anyhow::{Context, Result, anyhow};
use crossterm::event::KeyCode;
use crossterm::event::{Event, poll, read};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use derive_getters::Getters;
use im::{Vector, vector};
use menu::{Menu, MenuItem};
use ratatui::prelude::{Backend, Rect};
use regex::Regex;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime};

mod menu;
mod paragraph;

pub fn watch_and_report(filename: &str, dates: &dyn Fn() -> DateRange) -> Result<()> {
    let mut menu = create_menu()?;
    let mut app_display = RealAppScreen {
        terminal: ratatui::init(),
    };
    let mut storage = RealStorage {};
    let mut editor = RealEditor {};
    let mut app_state = WatchApp::new(
        filename,
        dates,
        &mut menu,
        &mut app_display,
        &mut storage,
        &mut editor,
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
}

trait AppScreen {
    fn read(&self, timeout: Duration) -> Result<ScreenEvent>;
    fn draw(&mut self, region_factory: &dyn Fn(Rect) -> Vector<Region>) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
}

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

trait Editor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()>;
}

struct RealAppScreen<T: Backend> {
    terminal: Terminal<T>,
}

impl<T: Backend> AppScreen for RealAppScreen<T> {
    fn read(&self, timeout: Duration) -> Result<ScreenEvent> {
        let error_context = "RealAppScreen.read";
        while poll(timeout).with_context(|| format!("{}: poll", error_context))? {
            match read().with_context(|| format!("{}: read", error_context))? {
                Event::Key(event) => match event.code {
                    KeyCode::Char(c) => return Ok(ScreenEvent::Char(c)),
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

    fn draw(&mut self, region_factory: &dyn Fn(Rect) -> Vector<Region>) -> Result<()> {
        let error_context = "RealAppScreen.draw";
        self.terminal
            .draw(|frame| {
                let regions = region_factory(frame.area());
                for region in regions.iter() {
                    frame.render_widget(region.paragraph.build(), region.area);
                }
            })
            .with_context(|| format!("{}: failed to draw terminal", error_context))
            .map(|_| ())
    }

    fn pause(&mut self) -> Result<()> {
        let error_context = "RealAppScreen.pause";
        disable_raw_mode()
            .with_context(|| format!("{}: failed to disable raw mode", error_context))?;
        self.terminal
            .clear()
            .with_context(|| format!("{}: failed to clear terminal", error_context))
    }

    fn resume(&mut self) -> Result<()> {
        let error_context = "RealAppScreen.resume";
        enable_raw_mode()
            .with_context(|| format!("{}: failed to enable raw mode", error_context))?;
        self.terminal
            .clear()
            .with_context(|| format!("{}: failed to clear terminal", error_context))
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

    fn load(&mut self, dates: DateRange, filename: &str) -> Result<LoadedFile> {
        let (day_entries, warnings) = parse::parse_file(filename)?;
        let min_date = dates.first().minus_days(30)?;
        let recent_projects = append::recent_projects(&day_entries, min_date, 5);
        let day_entries = report::day_entries_in_range(&dates, &day_entries);
        Ok(LoadedFile::new(
            dates,
            &day_entries,
            &warnings,
            &recent_projects,
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

#[derive(Clone, Getters)]
struct LoadedFile {
    dates: DateRange,
    day_entries: Vector<DayEntry>,
    warnings: Vector<String>,
    recent_projects: Vector<Project>,
}

impl LoadedFile {
    fn new(
        dates: DateRange,
        day_entries: &Vector<DayEntry>,
        warnings: &Vector<String>,
        recent_projects: &Vector<Project>,
    ) -> Self {
        LoadedFile {
            dates,
            day_entries: day_entries.clone(),
            warnings: warnings.clone(),
            recent_projects: recent_projects.clone(),
        }
    }

    fn empty(dates: DateRange) -> Self {
        LoadedFile {
            dates,
            day_entries: Vector::new(),
            warnings: Vector::new(),
            recent_projects: Vector::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Region {
    paragraph: ParagraphBuilder,
    area: Rect,
}

impl Region {
    fn new(paragraph: ParagraphBuilder, area: Rect) -> Self {
        Region { paragraph, area }
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

struct WatchApp<'a> {
    filename: &'a str,
    last_update_millis: u128,
    loaded: LoadedFile,
    read_timeout: Duration,
    update_delay_millis: u128,
    dates: &'a dyn Fn() -> DateRange,
    menu: &'a mut Menu<UserRequest>,
    app_screen: &'a mut dyn AppScreen,
    storage: &'a mut dyn Storage,
    editor: &'a mut dyn Editor,
}

impl<'a> WatchApp<'a> {
    fn new(
        filename: &'a str,
        dates: &'a dyn Fn() -> DateRange,
        menu: &'a mut Menu<UserRequest>,
        app_screen: &'a mut dyn AppScreen,
        storage: &'a mut dyn Storage,
        editor: &'a mut dyn Editor,
    ) -> WatchApp<'a> {
        WatchApp {
            filename,
            dates,
            loaded: LoadedFile::empty(dates()),
            last_update_millis: 0,
            update_delay_millis: 500,
            read_timeout: Duration::from_millis(100),
            menu,
            app_screen,
            storage,
            editor,
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

    fn update_screen(&mut self, what_to_display: &DisplayContent) -> Result<()> {
        match what_to_display {
            DisplayContent::Report(loaded_file) => self.app_screen.draw(&|screen_area| {
                create_report_screen(screen_area, self.menu, self.filename, loaded_file)
            }),
            DisplayContent::Warnings(loaded_file) => self
                .app_screen
                .draw(&|screen_area| create_warnings_screen(screen_area, self.menu, loaded_file)),
            DisplayContent::Error(error) => self.app_screen.draw(&|screen_area| {
                create_error_screen(screen_area, self.menu, self.filename, error)
            }),
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
        };
        result.or_else(|e| Ok(UICommand::DisplayError(e)))
    }

    fn change_menu_selection(&mut self, user_request: UserRequest) -> Result<UICommand> {
        match user_request {
            UserRequest::Left => self.menu.left(),
            UserRequest::Right => self.menu.right(),
            x => {
                return Err(anyhow!(
                    "WatchApp.change_menu_selection: invalid command({:?})",
                    x
                ));
            }
        };
        Ok(UICommand::UpdateMenu)
    }

    fn read_user_request(&mut self) -> Result<UserRequest> {
        loop {
            match self.app_screen.read(self.read_timeout)? {
                ScreenEvent::Char(c) => match self.menu.select(c) {
                    Some(x) => return Ok(x),
                    None => continue,
                },
                ScreenEvent::Enter => return Ok(self.menu.value()),
                ScreenEvent::Left => return Ok(UserRequest::Left),
                ScreenEvent::Right => return Ok(UserRequest::Right),
                ScreenEvent::Timeout => return Ok(UserRequest::Timeout),
                ScreenEvent::Resized => return Ok(UserRequest::Resized),
            }
        }
    }

    fn load(&mut self, force_reload: bool) -> Result<UICommand> {
        let current_file_millis = self.storage.timestamp(self.filename)?;
        if current_file_millis == self.last_update_millis && !force_reload {
            return Ok(UICommand::DoNothing);
        }
        let next_update_millis = self.last_update_millis + self.update_delay_millis;
        if current_file_millis < next_update_millis && !force_reload {
            return Ok(UICommand::DoNothing);
        }
        let dates = self.date_range();
        self.last_update_millis = current_file_millis;
        self.loaded = self.storage.load(dates, self.filename)?;
        if self.loaded.day_entries.is_empty() {
            self.loaded
                .warnings
                .push_front(format!("No day entries found in date range: {}.", dates));
        }
        Ok(UICommand::Report(self.loaded.clone()))
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
        let line_number = self
            .loaded
            .day_entries()
            .last()
            .map(|e| e.line_number())
            .unwrap_or(&0);

        self.app_screen
            .pause()
            .with_context(|| "failed to pause to run editor")?;
        let rc = self
            .editor
            .edit_file(self.filename, *line_number)
            .and_then(|_| self.load(true));
        _ = self.app_screen.resume();
        rc
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
    for (index, item) in menu.items().iter().enumerate() {
        let selected = index == *menu.selected_index();
        let style = menu_style(selected);
        builder
            .add_styled(item.display().to_string(), style)
            .add_plain("  ".to_string());
    }

    builder
        .new_line()
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
        _ => format!("There are {} warnings.", file.warnings.len()),
    };
    let mut builder = ParagraphBuilder::new();
    builder
        .add_styled(text, style)
        .new_line()
        .titled("Warnings".to_string());
    builder
}

fn format_warnings(file: &LoadedFile) -> ParagraphBuilder {
    let mut builder = ParagraphBuilder::new();
    if file.warnings.is_empty() {
        builder
            .add_plain("There are no warnings to display.".to_string())
            .new_line();
    } else {
        let style = Style::new().fg(Color::Red);
        for warning in file.warnings.iter() {
            builder
                .add_styled(format!("warning: {}", warning), style)
                .new_line();
        }
    }
    builder.bordered();
    builder
}

fn format_report(file: &LoadedFile) -> Result<ParagraphBuilder> {
    let mut builder = ParagraphBuilder::new();
    for line in report::create_report(file.dates, &file.day_entries)? {
        builder.add_plain(line).new_line();
    }
    builder.titled("Report".to_string());
    Ok(builder)
}

fn format_error(filename: &str, error: &anyhow::Error) -> ParagraphBuilder {
    let style = Style::new().fg(Color::Red);
    let lines = format!("{:?}", error)
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut builder = ParagraphBuilder::new();
    builder.add_styled(format!("   filename: {}", filename), style);
    builder
        .add_styled(
            format!(
                "    message: {:?}",
                lines.first().map(|s| s.as_ref()).unwrap_or("")
            ),
            style,
        )
        .new_line();
    for line in lines.iter().skip(1) {
        builder
            .add_styled(format!("           : {}", line), style)
            .new_line();
    }
    builder
}

fn create_warnings_screen(
    screen_area: Rect,
    menu: &Menu<UserRequest>,
    file: &LoadedFile,
) -> Vector<Region> {
    use Constraint::{Length, Min};
    let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0)]);
    let [menu_area, warnings_area] = vertical.areas(screen_area);
    vector!(
        Region::new(format_menu(menu), menu_area),
        Region::new(format_warnings(file), warnings_area)
    )
}

fn create_error_screen(
    screen_area: Rect,
    menu: &Menu<UserRequest>,
    filename: &str,
    error: &anyhow::Error,
) -> Vector<Region> {
    use Constraint::{Length, Min};
    let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0)]);
    let [menu_area, error_area] = vertical.areas(screen_area);
    vector!(
        Region::new(format_menu(menu), menu_area),
        Region::new(format_error(filename, error), error_area)
    )
}

fn create_report_screen(
    screen_area: Rect,
    menu: &Menu<UserRequest>,
    filename: &str,
    file: &LoadedFile,
) -> Vector<Region> {
    let report = match format_report(file) {
        Ok(r) => r,
        Err(e) => return create_error_screen(screen_area, menu, filename, &e),
    };
    use Constraint::{Length, Min};
    let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0), Length(WARNING_HEIGHT)]);
    let [menu_area, report_area, warnings_area] = vertical.areas(screen_area);
    vector!(
        Region::new(format_menu(menu), menu_area),
        Region::new(report, report_area),
        Region::new(format_warnings_summary(file), warnings_area)
    )
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
}
