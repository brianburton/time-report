use crate::model::{Date, DateRange, DayEntry, Project, ProjectTimes, Time, TimeRange};
use im::{HashMap, OrdSet, Vector, hashmap, ordset, vector};
use lazy_static::lazy_static;
use rand::prelude::*;

lazy_static! {
    static ref PROJECTS: Vector<Project> = vector!(
        Project::new("nasa", "navigation system"),
        Project::new("nasa", "saturn v launch"),
        Project::new("nasa", "astronaut recovery"),
        Project::new("nasa", "monkey training"),
        Project::new("nasa", "meeting"),
        Project::new("spacex", "elon meeting"),
        Project::new("spacex", "landing software"),
        Project::new("spacex", "navigation"),
        Project::new("spacex", "pr meeting"),
        Project::new("blue", "jeff meeting"),
        Project::new("blue", "aws interop"),
        Project::new("blue", "navigation fixes"),
        Project::new("carnival", "gps upgrade"),
        Project::new("carnival", "hull scrub"),
        Project::new("carnival", "lifeboat repairs"),
        Project::new("carnival", "band auditions")
    );
    static ref EIGHT_AM: Time = Time::new(8, 0).unwrap();
    static ref NOON: Time = Time::new(12, 0).unwrap();
    static ref ONE_PM: Time = Time::new(13, 0).unwrap();
    static ref FIVE_PM: Time = Time::new(17, 0).unwrap();
    static ref LUNCH_HOUR: TimeRange = TimeRange::new(*NOON, *ONE_PM).unwrap();
}

pub struct Random {
    rng: Box<dyn RngCore>,
}

impl Random {
    pub fn new() -> Random {
        Random {
            rng: Box::new(rand::rng()),
        }
    }

    pub fn for_testing() -> Random {
        type SeedType = <StdRng as SeedableRng>::Seed;
        let seed: SeedType = [42u8; 32];
        let rng = StdRng::from_seed(seed);
        Random { rng: Box::new(rng) }
    }

    pub fn next_index(&mut self, limit: usize) -> usize {
        self.rng.random_range(0..limit)
    }

    pub fn pick_one<'a, T: Clone>(&mut self, v: &'a Vector<T>) -> &'a T {
        let i = &self.rng.random_range(0..v.len());
        &v[*i]
    }

    pub fn next_time(&mut self) -> Time {
        let h = if self.next_index(10) < 5 {
            8 + self.next_index(4)
        } else {
            13 + self.next_index(4)
        };
        let m = self.next_index(60);
        Time::new(h as u16, m as u16).unwrap()
    }

    pub fn inbound(&mut self, bound: usize, chances: usize) -> bool {
        let roll = 1 + self.next_index(chances);
        roll <= bound
    }
}

fn update_random_projects<'a>(
    rnd: &mut Random,
    projects: &Vector<&'a Project>,
    target_len: usize,
) -> Vector<&'a Project> {
    let mut answer = projects.clone();
    while answer.len() <= target_len {
        let add_me = rnd.pick_one(&PROJECTS);
        answer = add_uniquely(&answer, add_me)
    }
    let remove_me = rnd.next_index(answer.len());
    answer.remove(remove_me);
    answer
}

fn add_uniquely<'a, T: Clone + PartialEq>(items: &Vector<&'a T>, item: &'a T) -> Vector<&'a T> {
    let mut answer = items.clone();
    for x in items.iter() {
        if *x == item {
            return answer;
        }
    }
    answer.push_back(item);
    answer
}

pub fn random_day_entries(rnd: &mut Random, dates: DateRange) -> Vector<DayEntry> {
    let project_count = 4;
    let mut answer: Vector<DayEntry> = vector!();
    let mut projects: Vector<&Project> = update_random_projects(rnd, &Vector::new(), project_count);
    assert_eq!(project_count, projects.len());
    for d in dates.iter() {
        if d.is_monday() {
            projects = update_random_projects(rnd, &projects, project_count);
            assert_eq!(project_count, projects.len());
        }
        answer.push_back(random_day_entry(rnd, d, &projects));
    }
    answer
}

fn random_day_entry(rnd: &mut Random, day: Date, projects: &Vector<&Project>) -> DayEntry {
    let time_ranges = random_time_ranges(rnd);
    let project_times = random_project_times(rnd, projects, &time_ranges);
    DayEntry::new(day, &project_times)
}

fn random_project_times(
    rnd: &mut Random,
    projects: &Vector<&Project>,
    time_ranges: &Vector<TimeRange>,
) -> Vector<ProjectTimes> {
    let mut cache: HashMap<&Project, Vector<TimeRange>> = hashmap!();
    let mut project = *rnd.pick_one(projects);
    for time_range in time_ranges.iter() {
        if rnd.inbound(1, 4) {
            project = *rnd.pick_one(projects)
        };
        if let Some(pt) = cache.get_mut(&project) {
            pt.push_back(*time_range);
        } else {
            cache.insert(project, vector!(*time_range));
        }
    }
    let mut answer = vector!();
    for (p, t) in cache.iter() {
        answer.push_back(ProjectTimes::new((**p).clone(), t).unwrap())
    }
    answer
}

fn random_time_ranges(rnd: &mut Random) -> Vector<TimeRange> {
    let mut times = random_times(rnd);
    combine_adjacent_times(&mut times)
}

fn random_times(rnd: &mut Random) -> OrdSet<Time> {
    let num_times = 2 + rnd.next_index(5);
    let mut times: OrdSet<Time> = ordset!(*EIGHT_AM, *NOON, *ONE_PM, *FIVE_PM);
    for _ in 0..num_times {
        times.insert(rnd.next_time());
    }
    times
}

fn combine_adjacent_times(times: &mut OrdSet<Time>) -> Vector<TimeRange> {
    let mut ranges: Vector<TimeRange> = Vector::new();
    let mut prev: Option<Time> = None;
    for time in times.iter() {
        match prev {
            None => {
                prev = Some(*time);
            }
            Some(p) => {
                let range = TimeRange::new(p, *time).unwrap();
                if range != *LUNCH_HOUR {
                    ranges.push_back(range);
                }
                prev = Some(*time);
            }
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add_uniquely() {
        let mut y: Vector<&i32> = add_uniquely(&Vector::new(), &1);
        assert_eq!(vector!(&1), y);

        y = add_uniquely(&y, &1);
        assert_eq!(vector!(&1), y);

        y = add_uniquely(&y, &2);
        assert_eq!(vector!(&1, &2), y);

        y = add_uniquely(&y, &1);
        assert_eq!(vector!(&1, &2), y);

        y = add_uniquely(&y, &2);
        assert_eq!(vector!(&1, &2), y);

        y = add_uniquely(&y, &3);
        assert_eq!(vector!(&1, &2, &3), y);
    }
}
