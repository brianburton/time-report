use super::*;
use crate::model::Project;
use im::vector;

fn time(h: u16, m: u16) -> Time {
    Time::new(h, m).unwrap()
}

fn time_range(h1: u16, m1: u16, h2: u16, m2: u16) -> TimeRange {
    TimeRange::new(time(h1, m1), time(h2, m2)).unwrap()
}

#[test]
fn test_parse_time_ranges() {
    let time_range_str = "0800-1200,1300-1310,1318-1708";
    let expected = vector![
        time_range(8, 0, 12, 0),
        time_range(13, 0, 13, 10),
        time_range(13, 18, 17, 8),
    ];

    assert_eq!(parse_time_ranges(time_range_str).unwrap().0, expected);
}

#[test]
fn test_remove_comments() {
    assert_eq!("", remove_comments(""));
    assert_eq!("", remove_comments("--"));
    assert_eq!("", remove_comments("    --  ignored"));
    assert_eq!("xyz", remove_comments(" xyz   --  ignored"));
    assert_eq!("xyz", remove_comments(" xyz --first  --  second"));
}

#[test]
fn test_parse_date_line() {
    let line = "Date: Thursday 04/03/2025";
    let expected = Date::new(2025, 4, 3).unwrap();

    assert_eq!(parse_date_line(line).unwrap(), expected);
}

#[test]
fn test_parse_label_line() {
    let line = "abc,xyz: 0800-1200,1300-1310,1318-1708";
    let expected = ProjectTimes::new(
        Project::new("abc", "xyz"),
        &vector![
            time_range(8, 0, 12, 0),
            time_range(13, 0, 13, 10),
            time_range(13, 18, 17, 8),
        ],
    );
    assert_eq!(parse_time_line(line).unwrap().0, expected.unwrap());
}

#[test]
fn test_parse_file() {
    let file_content =
        "Date: Thursday 04/03/2025\n\nabc,xyz: 0800-1200,1300-1310,1318-1708\ndef,uvw: 1200-1300\n";
    let file_path = "test_file.txt";
    std::fs::write(file_path, file_content).unwrap();

    let expected = vector!(DayEntry::new(
        Date::new(2025, 4, 3).unwrap(),
        &vector!(
            ProjectTimes::new(
                Project::new("abc", "xyz"),
                &vector!(
                    time_range(8, 0, 12, 0),
                    time_range(13, 0, 13, 10),
                    time_range(13, 18, 17, 8),
                ),
            )
            .unwrap(),
            ProjectTimes::new(
                Project::new("def", "uvw"),
                &vector!(time_range(12, 0, 13, 0),),
            )
            .unwrap(),
        ),
    ));

    let result = parse_file(file_path).unwrap();

    assert_eq!(result.0, expected);

    std::fs::remove_file(file_path).unwrap(); // Clean up test file
}
