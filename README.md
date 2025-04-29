# time-report

KISS solution for time tracking.  Parses a text file containing a work time log
that can be used for billing purposes.  This `rust` program was a learning exercise
to replace a far more complex `ruby` script.

## Time Log Format

I used this text file format to track my billable hours for years when working as a consultant.

Each day in the time log starts with a `Date:` line followed by one or more
project lines.  Each project line starts with a client id, comma, and project id.
These are followed by a `:` and a series of comma separated start and stop times
in the form `hhmm-hhmm`.

```
Date: Thursday 07/04/2024
acme,cms: 0835-1155,1400-1500,1530-1810
bozon,prototype: 1205-1400,1810-2000

Date: Friday 07/05/2024
acme,cms: 0815-1415
bozon,prototype: 1515-1820
```

## Usage

The program requires two positional arguments, a command and a file name.

* `report`: Prints a report for the current semi-monthly period (1-15, 16+) based on the current date.
* `append`: Adds an entry at end of file for the current date with empty projects selected from the 5 most recently used projects.

The second argument, `filename` must be a valid (though possibly empty) time log file.
