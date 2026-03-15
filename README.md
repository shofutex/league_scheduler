# league_scheduler

Tool to select bye weeks and generate a fair schdule for a swim league.
Developed with the help of ChatGPT--which seems to account for the complex mappings.
It seems to work, but this is one of those scenarios where it's easy to double-check.
ChatGPT hardcoded the team names and such so I edited it to work off league.yaml.
I also cleaned up the code so it's more readable

### Usage:
Edit `league_example.yaml` and copy it into league.yaml
```bash
$ ./league_matchups.py
```

