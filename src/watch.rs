use ratatui::{
    Terminal,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
};

use crate::menu::{Menu, MenuItem};
use crate::model::{Date, DateRange, DayEntry, Project};
use crate::report;
use crate::{append, parse};
use anyhow::{Context, Result, anyhow};
use crossterm::event::KeyCode;
use crossterm::event::{Event, poll, read};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use derive_getters::Getters;
use im::{Vector, vector};
use ratatui::buffer::Buffer;
use ratatui::prelude::{Backend, Rect, Widget};
use regex::Regex;
use scopeguard::defer;
use std::env;
use std::fs;
use std::process::Command;
use std::time::{Duration, SystemTime};

pub fn watch_and_report(filename: &str, dates: &dyn Fn() -> DateRange) -> Result<()> {
    let terminal = ratatui::init();
    defer! {
        ratatui::restore();
    }

    let mut menu = create_menu()?;
    let mut app_display = RealAppScreen { terminal };
    let mut storage = RealStorage {};
    let mut editor = RealEditor {};
    let mut app_state = WatchApp::new(
        filename,
        &mut menu,
        &mut app_display,
        &mut storage,
        &mut editor,
    );
    app_state.run(dates)
}

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

trait AppScreen {
    fn read(&self, timeout: Duration) -> Result<RawReadResult>;
    fn draw(&mut self, region_factory: &dyn Fn(Rect) -> Vector<Region>) -> Result<()>;
    fn pause(&mut self) -> Result<()>;
    fn resume(&mut self) -> Result<()>;
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

trait Editor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()>;
}

struct RealAppScreen<T: Backend> {
    terminal: Terminal<T>,
}

impl<T: Backend> AppScreen for RealAppScreen<T> {
    fn read(&self, timeout: Duration) -> Result<RawReadResult> {
        let error_context = "RealAppScreen.read";
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

type SpanSpec = (String, Option<Style>);
type LineSpec = Vector<SpanSpec>;

#[derive(Debug, Clone, PartialEq)]
struct ParagraphBuilder {
    spans: Vector<SpanSpec>,
    lines: Vector<LineSpec>,
    border: Option<String>,
}

impl ParagraphBuilder {
    fn new() -> Self {
        Self {
            spans: vector!(),
            lines: vector!(),
            border: None,
        }
    }

    fn add_plain(&mut self, s: String) -> &mut Self {
        self.add((s, None))
    }

    fn add_styled(&mut self, s: String, style: Style) -> &mut Self {
        self.add((s, Some(style)))
    }

    fn add(&mut self, spec: SpanSpec) -> &mut Self {
        self.spans.push_back(spec);
        self
    }

    fn new_line(&mut self) -> &mut Self {
        self.lines.push_back(self.spans.clone());
        self.spans.clear();
        self
    }

    fn bordered(&mut self) -> &mut Self {
        self.border = Some(String::new());
        self
    }

    fn titled(&mut self, title: String) -> &mut Self {
        self.border = Some(title);
        self
    }

    fn build(&self) -> Paragraph {
        let lines: Vec<Line> = self
            .lines
            .iter()
            .map(|spec| Self::build_line(spec))
            .collect();
        let para = Paragraph::new(lines);
        match &self.border {
            Some(title) if title.is_empty() => para.block(Block::bordered()),
            Some(title) => para.block(Block::bordered().title(title.to_string())),
            None => para,
        }
    }

    fn build_line<'a>(spans: &'a Vector<SpanSpec>) -> Line<'a> {
        let spans: Vec<Span<'a>> = spans
            .iter()
            .map(|(t, s)| match s {
                Some(style) => Span::styled(t, *style),
                None => Span::raw(t),
            })
            .collect();
        Line::from(spans)
    }
}

impl Widget for ParagraphBuilder {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.build().render(area, buf)
    }
}

#[derive(Clone, Getters)]
struct LoadedFile {
    day_entries: Vector<DayEntry>,
    warnings: Vector<String>,
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

enum Displayed {
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
    menu: &'a mut Menu<ReadResult>,
    app_screen: &'a mut dyn AppScreen,
    storage: &'a mut dyn Storage,
    editor: &'a mut dyn Editor,
}

impl<'a> WatchApp<'a> {
    fn new(
        filename: &'a str,
        menu: &'a mut Menu<ReadResult>,
        app_screen: &'a mut dyn AppScreen,
        storage: &'a mut dyn Storage,
        editor: &'a mut dyn Editor,
    ) -> WatchApp<'a> {
        WatchApp {
            filename,
            loaded: LoadedFile::empty(),
            last_update_millis: 0,
            update_delay_millis: 500,
            read_timeout: Duration::from_millis(100),
            menu,
            app_screen,
            storage,
            editor,
        }
    }

    fn run(&mut self, dates: &dyn Fn() -> DateRange) -> Result<()> {
        let mut displayed = Displayed::Report(LoadedFile {
            day_entries: vector!(),
            warnings: vector!(),
        });
        loop {
            let outcome = self.run_once()?;
            match outcome {
                UICommand::Quit => return Ok(()),
                UICommand::DoNothing => continue,
                UICommand::Report(loaded) => displayed = Displayed::Report(loaded),
                UICommand::UpdateMenu => (),
                UICommand::DisplayWarnings(loaded) => displayed = Displayed::Warnings(loaded),
                UICommand::DisplayError(error) => displayed = Displayed::Error(error),
            };
            self.update_screen(&displayed, dates)?;
        }
    }

    fn update_screen(
        &mut self,
        displayed: &Displayed,
        dates: &dyn Fn() -> DateRange,
    ) -> Result<()> {
        match displayed {
            Displayed::Report(loaded_file) => self.app_screen.draw(&|screen_area| {
                create_report_screen(screen_area, self.menu, self.filename, loaded_file, dates())
            }),
            Displayed::Warnings(loaded_file) => self
                .app_screen
                .draw(&|screen_area| create_warnings_screen(screen_area, self.menu, loaded_file)),
            Displayed::Error(error) => self.app_screen.draw(&|screen_area| {
                create_error_screen(screen_area, self.menu, self.filename, error)
            }),
        }
    }

    fn run_once(&mut self) -> Result<UICommand> {
        match self.read()? {
            ReadResult::Quit => Ok(UICommand::Quit),
            ReadResult::Reload => match self.load(true) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Warnings => Ok(UICommand::DisplayWarnings(self.loaded.clone())),
            ReadResult::Edit => match self.edit() {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Append => match self.append() {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Left => {
                self.menu.left();
                Ok(UICommand::UpdateMenu)
            }
            ReadResult::Right => {
                self.menu.right();
                Ok(UICommand::UpdateMenu)
            }
            ReadResult::Resized => Ok(UICommand::Report(self.loaded.clone())),
            ReadResult::Timeout => match self.load(false) {
                Ok((true, loaded)) => Ok(UICommand::Report(loaded)),
                Ok((false, _)) => Ok(UICommand::DoNothing),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
        }
    }

    fn read(&mut self) -> Result<ReadResult> {
        loop {
            match self.app_screen.read(self.read_timeout)? {
                RawReadResult::Char(c) => match self.menu.select(c) {
                    Some(x) => return Ok(x),
                    None => continue,
                },
                RawReadResult::Enter => return Ok(self.menu.value()),
                RawReadResult::Left => return Ok(ReadResult::Left),
                RawReadResult::Right => return Ok(ReadResult::Right),
                RawReadResult::Timeout => return Ok(ReadResult::Timeout),
                RawReadResult::Resized => return Ok(ReadResult::Resized),
            }
        }
    }

    fn load(&mut self, skip_delay: bool) -> Result<(bool, LoadedFile)> {
        let current_file_millis = self.storage.timestamp(self.filename)?;
        if current_file_millis == self.last_update_millis {
            return Ok((false, self.loaded.clone()));
        }
        let next_update_millis = self.last_update_millis + self.update_delay_millis;
        if current_file_millis < next_update_millis && !skip_delay {
            return Ok((false, self.loaded.clone()));
        }
        self.last_update_millis = current_file_millis;
        self.loaded = self.storage.load(self.filename)?;
        Ok((true, self.loaded.clone()))
    }

    fn append(&mut self) -> Result<(bool, LoadedFile)> {
        self.load(true)?;
        let date = Date::today();
        let day_entries = self.loaded.day_entries();
        append::validate_date(day_entries, date)?;

        let min_date = date.minus_days(30)?;
        let recent_projects = append::recent_projects(day_entries, min_date, 5);
        self.storage.append(self.filename, date, recent_projects)?;
        self.load(true)
    }

    fn edit(&mut self) -> Result<(bool, LoadedFile)> {
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

fn partition_string<'a>(s: &'a str, search_str: &str) -> Vec<&'a str> {
    let search_str = search_str.to_lowercase().to_string();
    for (c_index, c) in s.char_indices() {
        if c.to_lowercase().to_string() == search_str {
            let c_end_index = c_index + c.len_utf8();
            return vec![&s[..c_index], &s[c_index..c_end_index], &s[c_end_index..]];
        }
    }
    vec![s, "", ""]
}

fn menu_style(selected: bool) -> Style {
    let color = if selected { Color::Red } else { Color::Blue };
    Style::new().fg(color)
}

fn add_menu_label<T: Copy>(
    builder: &mut ParagraphBuilder,
    menu_item: &MenuItem<T>,
    selected: bool,
) {
    let parts = partition_string(menu_item.name(), &menu_item.key().to_string());
    for (index, s) in parts.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let mut style = menu_style(selected);
        if index == 1 {
            style = style.add_modifier(Modifier::BOLD);
            builder.add_styled(format!("({})", s), style);
        } else {
            builder.add_styled(s.to_string(), style);
        }
    }
    builder.add_plain("   ".to_string());
}

fn format_menu<T: Copy>(menu: &Menu<T>) -> ParagraphBuilder {
    let mut builder = ParagraphBuilder::new();
    for (index, item) in menu.items().iter().enumerate() {
        add_menu_label(&mut builder, item, index == *menu.selected_index());
    }

    builder
        .new_line()
        .add_styled(format!(" {}", menu.description()), menu_style(true))
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

fn format_report(file: &LoadedFile, dates: DateRange) -> Result<ParagraphBuilder> {
    let mut builder = ParagraphBuilder::new();
    for line in report::create_report(dates, &file.day_entries)? {
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
    menu: &Menu<ReadResult>,
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
    menu: &Menu<ReadResult>,
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
    menu: &Menu<ReadResult>,
    filename: &str,
    file: &LoadedFile,
    dates: DateRange,
) -> Vector<Region> {
    let report = match format_report(file, dates) {
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

fn supports_line_num_arg(editor: &str) -> bool {
    Regex::new(r"^(.*/)?((vim?)|(hx))$")
        .unwrap()
        .is_match(editor)
}
