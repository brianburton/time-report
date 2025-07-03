use anyhow::{Result, anyhow};
use im::{HashMap, OrdSet, Vector};
use model::{Date, DateRange, DayEntry, Project};

use crate::model::{self, ProjectTimes};

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ReportMode {
    Detail,
    Summary,
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
struct Key {
    day_name: String,
    project: Project,
}

impl Key {
    fn new(project: &Project, day_name: &str) -> Key {
        Key {
            day_name: day_name.to_string(),
            project: project.clone(),
        }
    }

    fn from_project_times(project_times: &ProjectTimes, day_name: &str) -> Key {
        Key {
            day_name: day_name.to_string(),
            project: project_times.project().clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct WeekData {
    minutes: HashMap<Key, u32>,
}

impl WeekData {
    fn new() -> WeekData {
        WeekData {
            minutes: HashMap::new(),
        }
    }

    fn clear(&mut self) {
        self.minutes.clear();
    }

    fn add_day_entry(&mut self, day_entry: &DayEntry) {
        let day_name = day_entry.date().day_abbrev();
        day_entry.projects().iter().for_each(|p| {
            let key = Key::from_project_times(p, &day_name);
            let total = p.time_ranges().iter().map(|r| r.duration() as u32).sum();
            match self.minutes.get_mut(&key) {
                Some(m) => *m += total,
                None => {
                    self.minutes.insert(key, total);
                }
            };
        });
    }

    fn project_day_total(&self, project: &Project, day_name: &str) -> u32 {
        let key = &Key::new(project, day_name);
        self.minutes.get(key).copied().unwrap_or(0)
    }

    fn project_billable(&self, project: &Project) -> u32 {
        self.compute_total(|k| k.project == *project, |m| billable_minutes(*m))
    }

    fn project_total(&self, project: &Project) -> u32 {
        self.compute_total(|k| k.project == *project, |m| *m)
    }

    fn day_billable(&self, day_name: &str) -> u32 {
        self.compute_total(|k| k.day_name == day_name, |m| billable_minutes(*m))
    }

    fn day_total(&self, day_name: &str) -> u32 {
        self.compute_total(|k| k.day_name == day_name, |m| *m)
    }

    fn week_total(&self) -> u32 {
        self.compute_total(|_| true, |m| *m)
    }

    fn week_billable(&self) -> u32 {
        self.compute_total(|_| true, |m| billable_minutes(*m))
    }

    fn compute_total<F, M>(&self, filter: F, mapper: M) -> u32
    where
        F: Fn(&Key) -> bool,
        M: Fn(&u32) -> u32,
    {
        self.minutes
            .iter()
            .filter(|(k, _)| filter(k))
            .map(|(_, v)| v)
            .map(mapper)
            .sum()
    }
}

fn billable_minutes(m: u32) -> u32 {
    m - (m % 15)
}

fn unique_projects(day_entries: &Vector<DayEntry>) -> OrdSet<Project> {
    day_entries
        .iter()
        .flat_map(|e| e.projects().iter().map(|p| p.project().clone()))
        .collect()
}

#[derive(Debug)]
struct ReportData {
    weeks: HashMap<u32, WeekData>,
    projects: OrdSet<Project>,
    dates: DateRange,
    totals: WeekData,
    weekdays: usize,
}

pub fn create_report(
    dates: DateRange,
    day_entries: &Vector<DayEntry>,
    mode: ReportMode,
) -> Result<Vector<String>> {
    let data = compute_report_data(dates, day_entries, mode)?;
    let lines = render_report_data(&data)?;
    Ok(lines)
}

pub fn day_entries_in_range(dates: &DateRange, day_entries: &Vector<DayEntry>) -> Vector<DayEntry> {
    let mut result: Vector<DayEntry> = day_entries
        .iter()
        .filter(|e| dates.contains(e.date()))
        .cloned()
        .collect();
    result.sort_by(|a, b| a.date().cmp(b.date()));
    result
}

fn adjust_day_entry_for_mode(day_entry: &DayEntry, mode: ReportMode) -> DayEntry {
    match mode {
        ReportMode::Summary => day_entry.without_subcodes(),
        ReportMode::Detail => day_entry.clone(),
    }
}

fn compute_report_data(
    dates: DateRange,
    day_entries: &Vector<DayEntry>,
    report_mode: ReportMode,
) -> Result<ReportData> {
    let mut weeks = HashMap::<u32, WeekData>::new();
    let week_nums = dates.iter().map(|d| d.week_num()).collect::<OrdSet<u32>>();
    for w in week_nums {
        weeks.insert(w, WeekData::new());
    }
    let mut totals = WeekData::new();
    let mut current_data = WeekData::new();
    let mut current_week = dates.first().week_num();

    for entry in day_entries {
        let entry = adjust_day_entry_for_mode(entry, report_mode);
        let entry_week = entry.date().week_num();
        if entry_week != current_week {
            weeks.insert(current_week, current_data.clone());
            current_data.clear();
            current_week = entry_week;
        };
        totals.add_day_entry(&entry);
        current_data.add_day_entry(&entry);
    }

    weeks.insert(current_week, current_data);

    let projects = unique_projects(day_entries);
    let last_date = day_entries.last().map(|e| *e.date());
    let weekdays = last_date
        .map(|ld| {
            day_entries
                .iter()
                .filter(|e| *e.date() <= ld)
                .filter(|e| e.date().is_weekday())
                .count()
        })
        .unwrap_or(0);
    Ok(ReportData {
        weeks,
        totals,
        projects,
        dates,
        weekdays,
    })
}

fn render_project_label(project: &Project) -> String {
    if project.subcode().is_empty() {
        format!("{},{}", project.client(), project.code())
    } else {
        format!(
            "{},{},{}",
            project.client(),
            project.code(),
            project.subcode()
        )
    }
}

fn create_project_labels(projects: &OrdSet<Project>) -> Vector<String> {
    let mut labels: Vector<String> = projects.iter().map(render_project_label).collect();
    let width = 4 + labels.iter().map(|label| label.len()).max().unwrap_or(0);
    labels.push_front("PROJECT".to_string());
    labels.push_front("".to_string());
    labels.push_back("TOTALS".to_string());
    labels.push_back("REPORT".to_string());
    labels
        .iter()
        .map(|s| format!("{:<w$}", s, w = width))
        .collect()
}

fn create_day_labels() -> String {
    "     MON     TUE     WED     THU     FRI     SAT     SUN".to_string()
}

const COLUMN_PAD: usize = 3;

fn render_dates_line(monday: Date) -> Result<String> {
    let mut line = "".to_string();
    let mut d = monday;
    loop {
        line += format!(
            "{:pad$}{:02}/{:02}",
            "",
            d.month(),
            d.day(),
            pad = COLUMN_PAD
        )
        .as_ref();
        if d.is_sunday() {
            break;
        }
        d = d.next()?;
    }
    line += "   TOTALS  REPORT";
    Ok(line)
}

fn render_time(minutes: u32, hour_len: usize) -> String {
    if minutes == 0 {
        format!("{:>width$}", "-", width = hour_len + 3)
    } else {
        format!(
            "{:>width$}:{:02}",
            minutes / 60,
            minutes % 60,
            width = hour_len
        )
    }
}

fn render_delta(delta_minutes: i32, hour_len: usize) -> String {
    let minutes = delta_minutes.abs();
    if minutes == 0 {
        format!("{:>width$}", "-", width = hour_len + 3)
    } else {
        format!(
            "{}{:>width$}:{:02}",
            if delta_minutes < 0 { "-" } else { "+" },
            minutes / 60,
            minutes % 60,
            width = hour_len - 1
        )
    }
}

fn render_times_line(monday: Date, project: &Project, week_data: &WeekData) -> Result<String> {
    let mut line = "".to_string();
    let mut d = monday;
    loop {
        let minutes = week_data.project_day_total(project, &d.day_abbrev());
        let time = render_time(minutes, 2);
        line += format!("{:pad$}{}", "", time, pad = COLUMN_PAD).as_ref();
        if d.is_sunday() {
            break;
        }
        d = d.next()?;
    }
    let total_time = render_time(week_data.project_total(project), 3);
    let total_billable = render_time(week_data.project_billable(project), 3);
    line += format!(
        "{:pad$}{}  {}",
        "",
        total_time,
        total_billable,
        pad = COLUMN_PAD
    )
    .as_ref();
    Ok(line)
}

fn render_totals_line(monday: Date, week_data: &WeekData) -> Result<String> {
    let mut line = "".to_string();
    let mut d = monday;
    loop {
        let minutes = week_data.day_total(&d.day_abbrev());
        let time = render_time(minutes, 2);
        line += format!("{:pad$}{}", "", time, pad = COLUMN_PAD).as_ref();
        if d.is_sunday() {
            break;
        }
        d = d.next()?;
    }
    let total_minutes = week_data.week_total();
    let total_time = render_time(total_minutes, 3);
    line += format!("{:pad$}{}", "", total_time, pad = COLUMN_PAD).as_ref();
    Ok(line)
}

fn render_billables_line(monday: Date, week_data: &WeekData) -> Result<String> {
    let mut line = "".to_string();
    let mut d = monday;
    loop {
        let minutes = week_data.day_billable(&d.day_abbrev());
        let time = render_time(minutes, 2);
        line += format!("{:pad$}{}", "", time, pad = COLUMN_PAD).as_ref();
        if d.is_sunday() {
            break;
        }
        d = d.next()?;
    }
    let total_minutes = week_data.week_billable();
    let total_time = render_time(total_minutes, 3);
    line += format!("{:pad$}{}", "", total_time, pad = COLUMN_PAD).as_ref();
    Ok(line)
}

fn render_grand_totals(
    projects: &OrdSet<Project>,
    totals_data: &WeekData,
    expected_time: u32,
) -> Vector<String> {
    let mut answer = Vector::new();
    let label_width = 3 + projects
        .iter()
        .map(|p| p.client().len() + p.code().len())
        .max()
        .unwrap_or(0);
    answer.push_back("".to_string());
    answer.push_back("".to_string());
    answer.push_back(format!(
        "{:lw$}{:pad$}{}{:pad$}{}",
        "PROJECT",
        "",
        "TOTALS",
        "",
        "REPORT",
        lw = label_width,
        pad = COLUMN_PAD
    ));
    for p in projects {
        answer.push_back(format!(
            "{:lw$}{:pad$}{:6}{:pad$}{:6}",
            format!("{},{}", p.client(), p.code()),
            "",
            render_time(totals_data.project_total(p), 3),
            "",
            render_time(totals_data.project_billable(p), 3),
            lw = label_width,
            pad = COLUMN_PAD,
        ));
    }
    answer.push_back(format!(
        "{:lw$}{:pad$}{}",
        "TOTALS",
        "",
        render_time(totals_data.week_total(), 3),
        lw = label_width,
        pad = COLUMN_PAD
    ));
    answer.push_back(format!(
        "{:lw$}{:pad$}{}",
        "REPORT",
        "",
        render_time(totals_data.week_billable(), 3),
        lw = label_width,
        pad = COLUMN_PAD
    ));
    let delta: i32 = (totals_data.week_billable() as i32) - (expected_time as i32);
    answer.push_back(format!(
        "{:lw$}{:pad$}{}",
        "DELTA",
        "",
        render_delta(delta, 3),
        lw = label_width,
        pad = COLUMN_PAD
    ));
    answer
}

fn render_report_data(report_data: &ReportData) -> Result<Vector<String>> {
    let mut answer = Vector::new();

    let left_labels = create_project_labels(&report_data.projects);
    let full_range = report_data.dates.as_full_weeks()?;
    for d in full_range {
        if !d.is_monday() {
            continue;
        }
        if !answer.is_empty() {
            answer.push_back("".to_string());
            answer.push_back("".to_string());
        }
        let mut i = 0;
        answer.push_back(format!("{}{}", left_labels[i], create_day_labels()));
        i += 1;
        answer.push_back(format!("{}{}", left_labels[i], render_dates_line(d)?));
        let week_data = if let Some(w) = report_data.weeks.get(&d.week_num()) {
            w
        } else {
            return Err(anyhow!("render_report_data: unable to find week data!"));
        };
        for p in report_data.projects.iter() {
            i += 1;
            answer.push_back(format!(
                "{}{}",
                left_labels[i],
                render_times_line(d, p, week_data)?
            ));
        }
        i += 1;
        answer.push_back(format!(
            "{}{}",
            left_labels[i],
            render_totals_line(d, week_data)?
        ));
        i += 1;
        answer.push_back(format!(
            "{}{}",
            left_labels[i],
            render_billables_line(d, week_data)?
        ));
    }
    let expected_time = 480 * report_data.weekdays;
    answer.append(render_grand_totals(
        &report_data.projects,
        &report_data.totals,
        expected_time as u32,
    ));
    Ok(answer)
}
