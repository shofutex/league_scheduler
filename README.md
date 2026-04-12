# league_scheduler

Tool to select bye weeks and generate a fair schdule for a 5- or 6-team swim league over (initially) a 5-week summer season.

### Rust

The Python version was imported into Claude and I worked with Claude to make a compiled application
to run on a user's system. It can save and load the preferences as a JSON file (Claude said that 
JSON is a better selection for Rust).

#### Usage
I've updated everything so now it's available cross-platform through the github release. Getting it to run on MacOS is a bit difficult, but it works.

On Linux:
```bash
$ chmod u+x swim-scheduler-linux
$ ./swim-scheduler-linux
```
On Mac OS:
```bash
$ chmod u+x swim-scheduler-mac
$ ./swim-scheduler-mac
```

On Windows, you can just double-click swim-scheduler-windows.exe


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


