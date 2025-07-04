# time-report

KISS solution for time tracking.  Parses a text file containing a work time log
that can be used for billing purposes.  This `rust` program was a learning exercise
to replace a far more complex `ruby` script.

## Time Log Format

I used this text file format to track my billable hours for years when working as a consultant.

Each day in the time log starts with a `Date:` line followed by one or more
project lines.  Each project line starts with a client id, comma, and project id.
An optional sub-project id can be provided as well.  Sub-projects are reported separately
in Detail report mode but aggregated under the project code in Summary report mode.
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
* `random`: Generates and prints a random time file to `stdout`.
* `watch`: Interactive mode that monitors the file for changes.  See below for details.

The second argument, `filename` must be a valid (though possibly empty) time log file.

## Watch Mode

Watch mode runs interactively.  It prints the current report to the terminal and monitors
the file looking for any changes.  If a change is detected it automatically regenerates
the report in near real time.

It also listens for and responds to single character commands:

* `q`: Exits the program immediately.
* `r`: Reloads and prints the report immediately.
* `a`: Appends the current date to the file then reloads and displays the report.
* `e`: Opens the file in the user's editor.  Reloads and displays the report when editor quits.
* `m`: Toggles between Summary and Detail report modes.

If the report is too long to fit in the window you can scroll:

* PageUp or Meta-v: Up several lines.
* PageDown or Control-v: Down several lines.
* Up Arrow: Up one line.
* Down Arrow: Down one line.
