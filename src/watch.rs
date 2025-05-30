use ratatui::{
    Frame, Terminal,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph},
};

use crate::menu::{Menu, MenuItem};
use crate::model::{Date, DateRange, DayEntry, Project};
use crate::report;
use crate::{append, parse};
use anyhow::{Context, Result, anyhow};
use crossterm::cursor::{Hide, Show};
use crossterm::event::KeyCode;
use crossterm::event::{Event, poll, read};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{QueueableCommand, cursor, execute, style, terminal};
use derive_getters::Getters;
use im::{Vector, vector};
use log::warn;
use ratatui::buffer::Buffer;
use ratatui::prelude::{Backend, Rect, Widget};
use regex::Regex;
use scopeguard::defer;
use std::fmt::Display;
use std::fs;
use std::fs::File;
use std::io::{Stdout, Write, stdout};
use std::process::Command;
use std::time::{Duration, SystemTime};
use std::{env, num::NonZero};

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

trait AppInput {
    fn read(&self, timeout: Duration) -> Result<RawReadResult>;
}

trait AppDisplay {
    fn clear(&mut self) -> Result<()>;
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

trait AppLogic {
    fn run(
        &mut self,
        menu: &mut Menu<ReadResult>,
        app_input: &mut dyn AppInput,
        app_screen: &mut dyn AppDisplay,
        storage: &mut dyn Storage,
        editor: &mut dyn Editor,
    ) -> Result<UICommand>;
}

trait Editor {
    fn edit_file(&self, filename: &str, line_number: u32) -> Result<()>;
}

struct RealAppInput {}
impl RealAppInput {
    fn new() -> Self {
        Self {}
    }
}

impl AppInput for RealAppInput {
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
        app_screen: &mut dyn AppDisplay,
        editor: &mut dyn Editor,
    ) -> Result<(bool, LoadedFile)> {
        let line_number = self
            .loaded
            .day_entries()
            .last()
            .map(|e| e.line_number())
            .unwrap_or(&0);

        app_screen.pause();
        // disable_raw_mode()?;
        // terminal.clear()?;
        defer! {
            // _=enable_raw_mode();
            app_screen.resume();
        }

        editor.edit_file(self.filename, *line_number)?;
        self.load(storage, true)
    }

    fn read(&mut self, menu: &mut Menu<ReadResult>, terminal: &dyn AppInput) -> Result<ReadResult> {
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
        app_input: &mut dyn AppInput,
        app_screen: &mut dyn AppDisplay,
        storage: &mut dyn Storage,
        editor: &mut dyn Editor,
    ) -> Result<UICommand> {
        match self.read(menu, app_input)? {
            ReadResult::Quit => Ok(UICommand::Quit),
            ReadResult::Reload => match self.load(storage, true) {
                Ok((_, loaded)) => Ok(UICommand::Report(loaded)),
                Err(e) => Ok(UICommand::DisplayError(e)),
            },
            ReadResult::Warnings => Ok(UICommand::DisplayWarnings(self.loaded.clone())),
            ReadResult::Edit => match self.edit(storage, app_screen, editor) {
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

#[derive(Clone, Debug, PartialEq)]
struct LineBuilder {
    spans: Vector<(String, Option<Style>)>,
}

impl LineBuilder {
    fn new() -> Self {
        Self { spans: vector!() }
    }

    fn from(text: String, style: Option<Style>) -> Self {
        let mut builder = LineBuilder::new();
        builder.spans.push_back((text, style));
        builder
    }

    fn add_plain(&mut self, s: String) -> &mut Self {
        self.spans.push_back((s, None));
        self
    }

    fn add_styled(&mut self, s: String, style: Style) -> &mut Self {
        self.spans.push_back((s, Some(style)));
        self
    }

    fn build(&self) -> Line {
        let spans: Vec<Span<'_>> = self
            .spans
            .iter()
            .map(|(t, s)| match s {
                Some(style) => Span::styled(t, *style),
                None => Span::raw(t),
            })
            .collect();
        Line::from(spans)
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ParagraphBuilder {
    lines: Vector<LineBuilder>,
    border: Option<String>,
}

impl ParagraphBuilder {
    fn new() -> Self {
        Self {
            lines: vector!(),
            border: None,
        }
    }

    fn add(&mut self, f: impl FnOnce(&mut LineBuilder)) -> &mut Self {
        let mut builder = LineBuilder::new();
        f(&mut builder);
        self.add_line(builder)
    }

    fn add_line(&mut self, line: LineBuilder) -> &mut Self {
        self.lines.push_back(line);
        self
    }

    fn add_line_str(&mut self, text: String, style: Option<Style>) -> &mut Self {
        self.add_line(LineBuilder::from(text, style))
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
        let lines: Vec<Line> = self.lines.iter().map(|line| line.build()).collect();
        let mut para = Paragraph::new(lines);
        match (&self.border) {
            Some(title) if title.is_empty() => para.block(Block::bordered()),
            Some(title) => para.block(Block::bordered().title(title.to_string())),
            None => para,
        }
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

enum Displayed {
    Report(LoadedFile),
    Warnings(LoadedFile),
    Error(anyhow::Error),
}

fn ui_impl(
    filename: &str,
    dates: &dyn Fn() -> DateRange,
    menu: &mut Menu<ReadResult>,
    app_input: &mut dyn AppInput,
    app_screen: &mut dyn AppDisplay,
    editor: &mut dyn Editor,
    storage: &mut dyn Storage,
    logic: &mut dyn AppLogic,
) -> Result<()> {
    let mut displayed = Displayed::Report(LoadedFile {
        day_entries: vector!(),
        warnings: vector!(),
    });
    loop {
        let outcome = logic.run(menu, app_input, app_screen, storage, editor)?;
        match outcome {
            UICommand::Quit => return Ok(()),
            UICommand::DoNothing => continue,
            UICommand::Report(loaded) => displayed = Displayed::Report(loaded),
            UICommand::UpdateMenu => (),
            UICommand::DisplayWarnings(loaded) => displayed = Displayed::Warnings(loaded),
            UICommand::DisplayError(error) => displayed = Displayed::Error(error),
        };
        _ = match &displayed {
            Displayed::Report(loaded_file) => app_screen.draw(&|screen_area| {
                create_report_screen(screen_area, menu, filename, &loaded_file, dates())
            })?,
            Displayed::Warnings(loaded_file) => app_screen
                .draw(&|screen_area| create_warnings_screen(screen_area, menu, &loaded_file))?,
            Displayed::Error(error) => app_screen
                .draw(&|screen_area| create_error_screen(screen_area, menu, filename, &error))?,
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

fn draw_screen(frame: &mut Frame, regions: &mut Vector<Region>) {
    for region in regions.iter() {
        frame.render_widget(region.paragraph.build(), region.area);
    }
}

struct RealAppDisplay<T: Backend> {
    terminal: Terminal<T>,
}

impl<T: Backend> AppDisplay for RealAppDisplay<T> {
    fn clear(&mut self) -> Result<()> {
        self.terminal
            .clear()
            .with_context(|| "failed to clear terminal")
    }

    fn draw(&mut self, region_factory: &dyn Fn(Rect) -> Vector<Region>) -> Result<()> {
        self.terminal
            .draw(|frame| {
                let mut regions = region_factory(frame.area());
                draw_screen(frame, &mut regions)
            })
            .with_context(|| "failed to draw terminal")
            .map(|_| ())
    }

    fn pause(&mut self) -> Result<()> {
        disable_raw_mode().with_context(|| "failed to disable raw mode")?;
        self.terminal
            .clear()
            .with_context(|| "failed to clear terminal")
    }

    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().with_context(|| "failed to enable raw mode")?;
        self.terminal
            .clear()
            .with_context(|| "failed to clear terminal")
    }
}

pub fn watch_and_report(filename: &str, dates: &dyn Fn() -> DateRange) -> Result<()> {
    let mut terminal = ratatui::init();
    defer! {
        ratatui::restore();
    }

    ui_impl(
        filename,
        dates,
        &mut create_menu()?,
        &mut RealAppInput::new(),
        &mut RealAppDisplay { terminal },
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

fn format_menu_label<T: Copy>(
    menu_item: &MenuItem<T>,
    selected: bool,
    line_builder: &mut LineBuilder,
) {
    let parts = partition_string(menu_item.name(), &*menu_item.key().to_string());
    for (index, s) in parts.iter().enumerate() {
        if s.is_empty() {
            continue;
        }
        let mut style = menu_style(selected);
        if index == 1 {
            style = style.add_modifier(Modifier::BOLD);
            line_builder.add_styled(format!("({})", s), style);
        } else {
            line_builder.add_styled(s.to_string(), style);
        }
    }
    line_builder.add_plain("   ".to_string());
}

fn format_menu<T: Copy>(menu: &Menu<T>) -> ParagraphBuilder {
    let mut choices = LineBuilder::new();
    for (index, item) in menu.items().iter().enumerate() {
        format_menu_label(item, index == *menu.selected_index(), &mut choices);
    }
    let mut builder = ParagraphBuilder::new();
    builder
        .add_line(choices)
        .add_line_str(menu.description().to_string(), Some(menu_style(true)))
        .bordered();
    builder
}

const MENU_HEIGHT: u16 = 4;
const WARNING_HEIGHT: u16 = 3;

fn format_warnings_summary(file: &LoadedFile) -> ParagraphBuilder {
    let style = Some(Style::new().fg(Color::Red));
    let text = match file.warnings.len() {
        0 => String::new(),
        1 => file.warnings.get(0).unwrap().clone(),
        _ => format!("There are {} warnings.", file.warnings.len()),
    };
    let mut builder = ParagraphBuilder::new();
    builder
        .add_line_str(text, style)
        .titled("Warnings".to_string());
    builder
}

fn format_warnings(file: &LoadedFile) -> ParagraphBuilder {
    let mut builder = ParagraphBuilder::new();
    if file.warnings.is_empty() {
        builder.add_line_str("There are no warnings to display.".to_string(), None);
    } else {
        let style = Some(Style::new().fg(Color::Red));
        for warning in file.warnings.iter() {
            builder.add_line_str(format!("warning: {}", warning), style);
        }
    }
    builder.bordered();
    builder
}

fn format_report(file: &LoadedFile, dates: DateRange) -> Result<ParagraphBuilder> {
    let mut builder = ParagraphBuilder::new();
    for line in report::create_report(dates, &file.day_entries)? {
        builder.add_line_str(line, None);
    }
    builder.titled("Report".to_string());
    Ok(builder)
}

fn format_error(filename: &str, error: &anyhow::Error) -> ParagraphBuilder {
    let style = Some(Style::new().fg(Color::Red));
    let lines = format!("{:?}", error)
        .lines()
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let mut builder = ParagraphBuilder::new();
    builder.add_line_str(format!("   filename: {}", filename), style);
    builder.add_line_str(
        format!(
            "    message: {:?}",
            lines.first().map(|s| s.as_ref()).unwrap_or("")
        ),
        style,
    );
    for line in lines.iter().skip(1) {
        builder.add_line_str(format!("           : {}", line), style);
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
        Region::new(format_error(filename, &error), error_area)
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
    if file.warnings().is_empty() {
        let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0)]);
        let [menu_area, report_area] = vertical.areas(screen_area);
        vector!(
            Region::new(format_menu(menu), menu_area),
            Region::new(report, report_area),
        )
    } else {
        let vertical = Layout::vertical([Length(MENU_HEIGHT), Min(0), Length(WARNING_HEIGHT)]);
        let [menu_area, report_area, warnings_area] = vertical.areas(screen_area);
        vector!(
            Region::new(format_menu(menu), menu_area),
            Region::new(report, report_area),
            Region::new(format_warnings_summary(file), warnings_area)
        )
    }
}

fn get_editor() -> String {
    env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
}

fn supports_line_num_arg(editor: &str) -> bool {
    Regex::new(r"^(.*/)?((vim?)|(hx))$")
        .unwrap()
        .is_match(editor)
}
