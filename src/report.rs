use im::{HashMap, OrdSet, Vector};
use model::{Date, DateRange, DayEntry};
use std::ops::Range;

use crate::{
    core::AppError,
    model::{self, ProjectTimes},
};

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Hash)]
struct Project {
    client: String,
    code: String,
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
            project: Project {
                client: project_times.client().to_string(),
                code: project_times.project().to_string(),
            },
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

    fn project_day_billable(&self, project: &Project, day_name: &str) -> u32 {
        let key = &Key::new(project, day_name);
        self.minutes
            .get(key)
            .map(|x| billable_minutes(*x))
            .unwrap_or(0)
    }

    fn project_day_total(&self, project: &Project, day_name: &str) -> u32 {
        let key = &Key::new(project, day_name);
        self.minutes.get(key).map(|x| *x).unwrap_or(0)
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
            .map(|m| mapper(m))
            .sum()
    }
}

fn billable_minutes(m: u32) -> u32 {
    m - (m % 15)
}

fn day_entries_in_range<'a>(
    dates: &DateRange,
    day_entries: &'a Vector<DayEntry>,
) -> Vector<&'a DayEntry> {
    let mut result: Vector<&'a DayEntry> = day_entries
        .iter()
        .filter(|e| dates.contains(e.date()))
        .collect();
    result.sort_by(|a, b| a.date().cmp(b.date()));
    result
}

fn unique_projects(day_entries: &Vector<&DayEntry>) -> OrdSet<Project> {
    day_entries
        .iter()
        .map(|e| {
            e.projects().iter().map(|p| Project {
                client: p.client().clone(),
                code: p.project().clone(),
            })
        })
        .flatten()
        .collect()
}

#[derive(Debug)]
struct ReportData {
    weeks: HashMap<u32, WeekData>,
    totals: WeekData,
    projects: OrdSet<Project>,
    dates: DateRange,
}

pub fn create_report(
    dates: DateRange,
    all_day_entries: &Vector<DayEntry>,
) -> Result<Vector<String>, AppError> {
    let data = compute_report_data(dates, all_day_entries)?;
    let lines = render_report_data(&data)?;
    Ok(lines)
}

fn compute_report_data<'a>(
    dates: DateRange,
    all_day_entries: &'a Vector<DayEntry>,
) -> Result<ReportData, AppError> {
    let day_entries = day_entries_in_range(&dates, all_day_entries);
    if day_entries.is_empty() {
        return Err(AppError::from_str(
            "report",
            "no data availble for date range",
        ));
    }

    let mut weeks = HashMap::<u32, WeekData>::new();
    let week_nums = dates.iter().map(|d| d.week_num()).collect::<OrdSet<u32>>();
    for w in week_nums {
        weeks.insert(w, WeekData::new());
    }
    let mut totals = WeekData::new();
    let mut current_data = WeekData::new();
    let mut current_week = dates.first().week_num();

    for entry in &day_entries {
        let entry_week = entry.date().week_num();
        if entry_week != current_week {
            weeks.insert(current_week, current_data.clone());
            current_data.clear();
            current_week = entry_week;
        };
        totals.add_day_entry(entry);
        current_data.add_day_entry(entry);
    }

    weeks.insert(current_week, current_data);

    let projects = unique_projects(&day_entries);
    Ok(ReportData {
        weeks,
        totals,
        projects,
        dates,
    })
}

fn create_project_labels(projects: &OrdSet<Project>) -> Vector<String> {
    let width = 4 + projects
        .iter()
        .map(|p| p.client.len() + p.code.len())
        .max()
        .unwrap_or(0);
    let mut labels: Vector<String> = projects
        .iter()
        .map(|p| format!("{},{}", p.client, p.code))
        .collect();
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

fn render_dates_line(monday: Date) -> Result<String, AppError> {
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
    line += "    TOTALS  REPORT";
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

fn render_times_line(
    monday: Date,
    project: &Project,
    week_data: &WeekData,
) -> Result<String, AppError> {
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

fn render_totals_line(monday: Date, week_data: &WeekData) -> Result<String, AppError> {
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

fn render_billables_line(monday: Date, week_data: &WeekData) -> Result<String, AppError> {
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

fn render_report_data(report_data: &ReportData) -> Result<Vector<String>, AppError> {
    let mut answer = Vector::new();

    let left_labels = create_project_labels(&report_data.projects);
    let full_range = report_data.dates.to_full_weeks()?;
    for d in full_range.iter() {
        if !d.is_monday() {
            continue;
        }
        if !answer.is_empty() {
            answer.push_back("".to_string());
        }
        let mut i = 0;
        answer.push_back(format!("{}{}", left_labels[i], create_day_labels()));
        i += 1;
        answer.push_back(format!("{}{}", left_labels[i], render_dates_line(d)?));
        let week_data = if let Some(w) = report_data.weeks.get(&d.week_num()) {
            w
        } else {
            return Err(AppError::from_str("report", "unable to find week data!"));
        };
        for p in report_data.projects.iter() {
            i += 1;
            answer.push_back(format!(
                "{}{}",
                left_labels[i],
                render_times_line(d, p, &week_data)?
            ));
        }
        i += 1;
        answer.push_back(format!(
            "{}{}",
            left_labels[i],
            render_totals_line(d, &week_data)?
        ));
        i += 1;
        answer.push_back(format!(
            "{}{}",
            left_labels[i],
            render_billables_line(d, &week_data)?
        ));
    }
    Ok(answer)
}
