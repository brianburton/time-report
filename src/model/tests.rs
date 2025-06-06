use super::*;
use im::{ordset, vector};

fn date(y: u16, m: u8, d: u8) -> Date {
    Date::new(y, m, d).unwrap()
}

fn time(h: u16, m: u16) -> Time {
    Time::new(h, m).unwrap()
}

fn time_range(h1: u16, m1: u16, h2: u16, m2: u16) -> TimeRange {
    TimeRange::new(time(h1, m1), time(h2, m2)).unwrap()
}

#[test]
fn test_time() {
    assert_eq!(4, std::mem::size_of::<Date>());
    assert_eq!(2, std::mem::size_of::<Time>());
    assert_eq!(0, time(0, 0).hour());
    assert_eq!(0, time(0, 59).hour());
    assert_eq!(1, time(1, 0).hour());
    assert_eq!(1, time(1, 59).hour());
    assert_eq!(23, time(23, 0).hour());
    assert_eq!(23, time(23, 59).hour());

    assert_eq!(0, time(0, 0).minute());
    assert_eq!(59, time(0, 59).minute());
    assert_eq!(0, time(1, 0).minute());
    assert_eq!(59, time(1, 59).minute());
    assert_eq!(0, time(23, 0).minute());
    assert_eq!(59, time(23, 59).minute());
}

#[test]
fn test_date_time_valid() {
    assert!(is_valid_time(0, 0));
    assert!(is_valid_time(23, 59));
    assert!(!is_valid_time(24, 0));
    assert!(!is_valid_time(0, 60));
    assert!(is_valid_date(MIN_YEAR, 1, 1));
    assert!(is_valid_date(MAX_YEAR, 12, 31));
    assert!(!is_valid_date(MIN_YEAR - 1, 12, 31));
    assert!(!is_valid_date(MAX_YEAR + 1, 1, 1));
    assert!(!is_valid_date(MAX_YEAR, 13, 1));
    assert!(!is_valid_date(MAX_YEAR, 1, 32));
}

#[test]
fn test_date_names() {
    assert_eq!("MON", Date::min_date().day_abbrev());
    assert_eq!("SUN", date(2025, 4, 6).day_abbrev());
    assert_eq!("MON", date(2025, 4, 7).day_abbrev());
    assert_eq!("TUE", date(2025, 4, 8).day_abbrev());
    assert_eq!("WED", date(2025, 4, 9).day_abbrev());
    assert_eq!("THU", date(2025, 4, 10).day_abbrev());
    assert_eq!("FRI", date(2025, 4, 11).day_abbrev());
    assert_eq!("SAT", date(2025, 4, 12).day_abbrev());
}

#[test]
fn test_day_number() {
    assert_eq!(0, day_number(MIN_YEAR, 1, 1));
    assert_eq!(365, day_number(MIN_YEAR + 1, 1, 1));

    let base = day_number(2000, 12, 31);
    assert_eq!(10226, base);

    let cases = [
        (1, 1, base + 1),
        (1, 31, base + 31),
        (2, 1, base + 32),
        (2, 28, base + 59),
        (3, 1, base + 60),
        (3, 31, base + 90),
        (4, 1, base + 91),
        (4, 30, base + 120),
        (5, 1, base + 121),
        (5, 31, base + 151),
        (6, 1, base + 152),
        (6, 30, base + 181),
        (7, 1, base + 182),
        (7, 31, base + 212),
        (8, 1, base + 213),
        (8, 31, base + 243),
        (9, 1, base + 244),
        (9, 30, base + 273),
        (10, 1, base + 274),
        (10, 31, base + 304),
        (11, 1, base + 305),
        (11, 30, base + 334),
        (12, 1, base + 335),
        (12, 31, base + 365),
    ];

    cases
        .iter()
        .for_each(|(m, d, num)| assert_eq!(*num, day_number(2001, *m, *d)));
}

#[test]
fn test_days_in_month() {
    [1, 3, 5, 7, 8, 10, 12]
        .iter()
        .for_each(|m| assert_eq!(31, days_in_month(2000, *m)));
    [4, 6, 9, 11]
        .iter()
        .for_each(|m| assert_eq!(30, days_in_month(2000, *m)));
    [2001, 2002, 2003, 2100, 2200, 2300]
        .iter()
        .for_each(|y| assert_eq!(28, days_in_month(*y, 2)));
    [1996, 2000, 2004]
        .iter()
        .for_each(|y| assert_eq!(29, days_in_month(*y, 2)));
}

#[test]
fn test_time_range_distinct() {
    // separated
    assert!(TimeRange::distinct(
        &time_range(1, 0, 3, 0),
        &time_range(5, 0, 7, 0),
    ));
    assert!(TimeRange::distinct(
        &time_range(5, 0, 7, 0),
        &time_range(1, 0, 3, 0),
    ));

    // adjacent
    assert!(TimeRange::distinct(
        &time_range(1, 0, 3, 0),
        &time_range(3, 0, 5, 0),
    ));
    assert!(TimeRange::distinct(
        &time_range(3, 0, 5, 0),
        &time_range(1, 0, 3, 0),
    ));

    // overlapping
    assert!(!TimeRange::distinct(
        &time_range(1, 0, 3, 0),
        &time_range(2, 59, 5, 0),
    ));
    assert!(!TimeRange::distinct(
        &time_range(3, 0, 5, 0),
        &time_range(1, 0, 3, 1),
    ));
    assert!(!TimeRange::distinct(
        &time_range(3, 0, 5, 0),
        &time_range(3, 1, 4, 59),
    ));
    assert!(!TimeRange::distinct(
        &time_range(3, 0, 5, 0),
        &time_range(3, 0, 4, 59),
    ));
    assert!(!TimeRange::distinct(
        &time_range(3, 0, 5, 0),
        &time_range(3, 1, 5, 0),
    ));
}

#[test]
fn test_find_overlapping_time_ranges() {
    let t13 = time_range(1, 0, 3, 0);
    let t24 = time_range(2, 0, 4, 0);
    let t34 = time_range(3, 0, 4, 0);
    let t35 = time_range(3, 0, 5, 0);
    let t56 = time_range(5, 0, 6, 0);
    let empty: Vector<TimeRange> = vector!();
    let no_match: OrdSet<TimeRange> = ordset!();

    assert_eq!(no_match, find_overlapping_time_ranges(&empty));
    assert_eq!(no_match, find_overlapping_time_ranges(&vector!(t13, t35)));
    assert_eq!(
        ordset!(t13, t35, t24),
        find_overlapping_time_ranges(&vector!(t13, t35, t24))
    );
    assert_eq!(
        no_match,
        find_overlapping_time_ranges(&vector!(t13, t35, t56))
    );
    assert_eq!(
        ordset!(t34, t35),
        find_overlapping_time_ranges(&vector!(t13, t34, t35, t56))
    );
}

#[test]
fn test_displays() {
    assert_eq!("0102", time(1, 2).to_string());
    assert_eq!("2359", time(23, 59).to_string());
    assert_eq!("0102-2359", time_range(1, 2, 23, 59).to_string());
    assert_eq!("01/02/1995", Date::new(1995, 1, 2).unwrap().to_string());
    assert_eq!("[1,2]", ordset_to_string(&ordset!(2, 1)));
}

#[test]
fn test_mondays() {
    assert!(!date(1996, 2, 25).is_monday());
    assert!(date(1996, 2, 26).is_monday());
    assert!(!date(1996, 2, 27).is_monday());
    assert!(!date(1996, 2, 28).is_monday());
    assert!(!date(1996, 2, 29).is_monday());
    assert!(!date(1996, 3, 1).is_monday());
    assert!(!date(1996, 3, 2).is_monday());
    assert!(!date(1996, 3, 3).is_monday());
    assert!(date(1996, 3, 4).is_monday());

    assert_eq!(date(1996, 2, 19), date(1996, 2, 25).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 26).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 27).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 28).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 29).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 1).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 2).this_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 3).this_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 3, 4).this_monday().unwrap());
    assert_eq!(date(1996, 12, 30), date(1997, 1, 5).this_monday().unwrap());

    assert_eq!(date(1996, 2, 19), date(1996, 2, 26).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 27).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 28).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 2, 29).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 1).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 2).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 3).prev_monday().unwrap());
    assert_eq!(date(1996, 2, 26), date(1996, 3, 4).prev_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 3, 11).prev_monday().unwrap());
    assert_eq!(date(1996, 12, 30), date(1997, 1, 6).prev_monday().unwrap());

    assert_eq!(date(1996, 3, 4), date(1996, 2, 26).next_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 2, 27).next_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 2, 28).next_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 2, 29).next_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 3, 1).next_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 3, 2).next_monday().unwrap());
    assert_eq!(date(1996, 3, 4), date(1996, 3, 3).next_monday().unwrap());
    assert_eq!(date(1996, 3, 11), date(1996, 3, 4).next_monday().unwrap());
    assert_eq!(date(1997, 1, 6), date(1996, 12, 30).next_monday().unwrap());
}

#[test]
fn test_sundays() {
    assert!(date(1996, 2, 25).is_sunday());
    assert!(!date(1996, 2, 26).is_sunday());
    assert!(!date(1996, 2, 27).is_sunday());
    assert!(!date(1996, 2, 28).is_sunday());
    assert!(!date(1996, 2, 29).is_sunday());
    assert!(!date(1996, 3, 1).is_sunday());
    assert!(!date(1996, 3, 2).is_sunday());
    assert!(date(1996, 3, 3).is_sunday());
    assert!(!date(1996, 3, 4).is_sunday());

    assert_eq!(date(1996, 2, 25), date(1996, 2, 25).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 2, 26).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 2, 27).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 2, 28).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 2, 29).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 3, 1).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 3, 2).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 3), date(1996, 3, 3).this_sunday().unwrap());
    assert_eq!(date(1996, 3, 10), date(1996, 3, 4).this_sunday().unwrap());
    assert_eq!(date(1997, 1, 5), date(1996, 12, 31).this_sunday().unwrap());
}

#[test]
fn test_date_next_prev() {
    assert_eq!(date(1996, 12, 31), date(1997, 1, 1).prev().unwrap());
    assert_eq!(date(1996, 1, 1), date(1996, 1, 2).prev().unwrap());
    assert_eq!(date(1996, 1, 31), date(1996, 2, 1).prev().unwrap());
    assert_eq!(date(1996, 2, 29), date(1996, 3, 1).prev().unwrap());

    assert_eq!(date(1997, 1, 1), date(1996, 12, 31).next().unwrap());
    assert_eq!(date(1996, 2, 1), date(1996, 1, 31).next().unwrap());
    assert_eq!(date(1996, 2, 29), date(1996, 2, 28).next().unwrap());
    assert_eq!(date(1996, 3, 1), date(1996, 2, 29).next().unwrap());
    assert_eq!(date(1996, 4, 1), date(1996, 3, 31).next().unwrap());
    assert_eq!(date(1996, 12, 1), date(1996, 11, 30).next().unwrap());
}

#[test]
fn test_date_iter() {
    let first = date(2000, 12, 28);
    let last = date(2001, 1, 2);
    let mut it = DateRange::new(first, last).iter();
    assert_eq!(Some(date(2000, 12, 28)), it.next());
    assert_eq!(Some(date(2000, 12, 29)), it.next());
    assert_eq!(Some(date(2000, 12, 30)), it.next());
    assert_eq!(Some(date(2000, 12, 31)), it.next());
    assert_eq!(Some(date(2001, 1, 1)), it.next());
    assert_eq!(Some(date(2001, 1, 2)), it.next());
    assert_eq!(None, it.next());
}
