# league_scheduler

Tool to select bye weeks and generate a fair schdule for a 5- or 6-team swim league over (initially) a 5-week summer season.

### Rust

The Python version was imported into Claude and I worked with Claude to make a compiled application
using the [iced framework](https://iced.rs/) so that it can generate a cross-platform GUI
to run on a user's system. It can save and load the preferences as a JSON file (Claude said that 
JSON is a better selection for Rust).

#### Usage
I've updated everything so now it's available cross-platform through the github release. Getting it to run on MacOS is a bit difficult (you'll have to go through the steps to allow this untrusted binary to run), but it works.

##### On Linux:
```bash
$ chmod u+x swim-scheduler-linux
$ ./swim-scheduler-linux
```
##### On Mac OS:
```bash
$ chmod u+x swim-scheduler-mac
```
Then you can double-click on it.

##### On Windows:

You can just double-click swim-scheduler-windows.exe

It is set up as a wizard so you can enter in all the requirements:
- Enter five or six team names
- Enter the number of weeks (it has only been tested with 5 at this time)
- Enter your preferred round-robin schedule with default team names like A, B, C, D, E (and F if needed). It can load defaults for you.
- If a team has a preferred bye-week, you can enter a 1st and 2nd choice (there are no guarantees the preferred bye-week will work out)
- If a team has a reason to *not* take a specific week as a bye-week, you can enter that as well
- If you have a team whose fairness score should be ignored, you can toggle that too
- You can save and load these configurations if you want to run it later

There are some known issues:
- Bye-weeks and preferences have not yet been tested with a 6-week schedule. The use-cases this was designed for do not have bye-weeks
for the 6-team schedules.
- Only 5-weeks have been tested, as the use-cases this was developed for are only 5-week long summer swim schedules
- 6-team schedules can take a significant number of seconds to run (there are 720 permutations to test for the 6-team/5-week schedule).

I need to set up a mechanism for exporting the findings.

### Python 
Developed with the help of ChatGPT--which seems to account for the complex mappings.
It seems to work, but this is one of those scenarios where it's easy to double-check.
ChatGPT hardcoded the team names and such so I edited it to work off league.yaml.
I also cleaned up the code so it's more readable

#### Usage:
Edit `league_example.yaml` and copy it into `league.yaml`
```bash
$ ./league_matchups.py
```


