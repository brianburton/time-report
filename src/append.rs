use crate::core::{create_temp_file, delete_file};
use crate::model::{Date, DayEntry, Project};
use anyhow::{Context, Result, anyhow};
use im::{HashMap, Vector};
use scopeguard::defer;
use std::cmp::Ordering;
use std::fs::File;
use std::io::{BufRead, Write};
use std::path::Path;
use std::{fs, io};

pub fn recent_projects(
    all_day_entries: &Vector<DayEntry>,
    min_date: Date,
    max_to_return: usize,
) -> Vector<Project> {
    let all_projects_map = recent_projects_with_date(all_day_entries, min_date);
    projects_sorted_by_date(all_projects_map, max_to_return)
        .into_iter()
        .cloned()
        .collect()
}

pub fn validate_date(all_day_entries: &Vector<DayEntry>, date: Date) -> Result<()> {
    for day in all_day_entries {
        match day.date().cmp(&date) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(anyhow!(
                    "append_to_file: date already in file: date='{}'",
                    date
                ));
            }
            Ordering::Greater => {
                return Err(anyhow!(
                    "append_to_file: newer date in file: date='{}' newer='{}'",
                    date,
                    day.date()
                ));
            }
        }
    }
    Ok(())
}

fn create_date_str(prev_blank: bool, date: Date, projects: &Vector<Project>) -> String {
    let mut s = String::new();
    if !prev_blank {
        s.push('\n');
    }
    s.push_str(format!("Date: {} {}\n", date.day_name(), date).as_str());
    projects
        .iter()
        .for_each(|p| s.push_str(format!("{},{}: \n", p.client(), p.code()).as_str()));
    s
}

pub fn append_to_file(filename: &str, date: Date, projects: &Vector<Project>) -> Result<()> {
    let error_context = "append_to_file";
    let temp_file = create_temp_file(filename)?;
    defer! { delete_file(&temp_file).unwrap_or(())}

    let input_path = Path::new(filename);
    let input_file =
        File::open(input_path).with_context(|| format!("{}: open failed", error_context))?;
    let reader = io::BufReader::new(input_file);

    let output_path = Path::new(&temp_file);
    let output_file =
        File::create(output_path).with_context(|| format!("{}: create failed", error_context))?;
    let mut writer = io::BufWriter::new(output_file);

    let mut appended = false;
    let mut prev_blank = true;
    for raw_line in reader.lines() {
        let line = raw_line.with_context(|| format!("{}: read failed", error_context))?;
        let trimmed = line.trim();
        if trimmed == "END" && !appended {
            writer
                .write_all(create_date_str(prev_blank, date, projects).as_bytes())
                .with_context(|| format!("{}: write failed", error_context))?;
            writer
                .write_all("\n".as_bytes())
                .with_context(|| format!("{}: write failed", error_context))?;
            appended = true;
        }
        writer
            .write_all(line.as_bytes())
            .with_context(|| format!("{}: write failed", error_context))?;
        writer
            .write_all("\n".as_bytes())
            .with_context(|| format!("{}: write failed", error_context))?;
        prev_blank = trimmed.is_empty();
    }
    if !appended {
        writer
            .write_all(create_date_str(prev_blank, date, projects).as_bytes())
            .with_context(|| format!("{}: write failed", error_context))?;
    }
    fs::rename(&temp_file, filename)
        .with_context(|| format!("{}: rename failed", error_context))?;
    Ok(())
}

fn projects_sorted_by_date<'a>(
    all_projects_and_dates_map: HashMap<&Project, (Date, &'a Project)>,
    max_to_return: usize,
) -> Vector<&'a Project> {
    let mut sorted_projects_and_dates = all_projects_and_dates_map.values().collect::<Vector<_>>();
    sorted_projects_and_dates.sort_by(|(d1, _), (d2, _)| d2.cmp(d1));
    sorted_projects_and_dates
        .iter()
        .take(max_to_return)
        .map(|(_, pr)| *pr)
        .collect::<Vector<_>>()
}

fn recent_projects_with_date(
    all_day_entries: &Vector<DayEntry>,
    min_date: Date,
) -> HashMap<&Project, (Date, &Project)> {
    all_day_entries
        .iter()
        .filter(|entry| *entry.date() >= min_date)
        .flat_map(|entry| {
            entry
                .projects()
                .iter()
                .map(|project_times| (*entry.date(), project_times.project()))
        })
        .fold(HashMap::new(), |acc, t| acc.update(t.1, t))
}
