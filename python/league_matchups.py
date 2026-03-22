#!/usr/bin/python3

# Tool to select bye weeks and generate a fair schedule for a swim league
# Developed with the help of ChatGPT--which seems to account for the complex mappings
# It seems to work, but this is one of those scenarios where it's easy to double-check
# ChatGPT hardcoded the team names and such so I edited it to work of league.yaml
# I also cleaned up the code so it's more readable

import yaml
import itertools
from collections import defaultdict


# Function to check that the bye assignments are valid
def valid_bye_assignment(bye_assignment):

    for team, week in bye_assignment.items():

        if team in bye_restrictions:
            if week in bye_restrictions[team]:
                return False

    return True

# Scores the byes in this chosen schedule for fairness
def score_byes(bye_assignment):

    score = 0
    detail = {}

    for team, week in bye_assignment.items():

        # Fair Oaks does not affect fairness score
        if team == "Fair Oaks":
            detail[team] = 0
            continue

        first, second = preferences[team]

        if week == first:
            score += 2
            detail[team] = 2
        elif week == second:
            score += 1
            detail[team] = 1
        else:
            detail[team] = 0

    return score, detail

# Assigns hosts for each matchup
def assign_hosts(schedule):

    options = []

    games = []
    for w in weeks:
        for g in schedule[w]:
            games.append((w,g))

    for combo in itertools.product([0,1], repeat=10):

        host_count = defaultdict(int)
        sched = {w:[] for w in weeks}

        for (w,(t1,t2)),c in zip(games,combo):

            host = t1 if c==0 else t2
            away = t2 if host==t1 else t1

            host_count[host]+=1
            sched[w].append((host,away))

        if all(host_count[t]==2 for t in teams):
            options.append(sched)

    return options

# Set up a penalty to try to avoid having a team host twice in a row
def host_streak_penalty(schedule):

    # build host week lists
    host_weeks = {t: [] for t in teams}

    for w in weeks:
        for host, away in schedule[w]:
            host_weeks[host].append(w)

    penalty = 0

    for t in teams:
        hw = sorted(host_weeks[t])

        for i in range(len(hw)-1):
            if hw[i+1] == hw[i] + 1:
                penalty += 1

    return penalty

## Main
if __name__ == "__main__":

    with open("league.yaml","r") as file:
        config = yaml.safe_load(file)

    # Load details from league.yaml 
    teams = config["league"]["teams"][:]

    weeks = config["league"]["weeks"][:]

    # bye preferences
    preferences = {}
    for pref in config["league"]["bye_preferences"]:
        prefs = config["league"]["bye_preferences"][pref]
        preferences[pref] = (prefs[0], prefs[1])

    # hard restrictions
    bye_restrictions = {}
    for restriction in config["league"]["bye_restrictions"]:
        bye_restrictions[restriction] = config["league"]["bye_restrictions"][restriction][:]

    # canonical 5-team round robin
    # based on what's in the yaml so it can be extended beyond 5 teams
    base_schedule={}
    for week in config["league"]["base_schedule"]:
        matchups = config["league"]["base_schedule"][week]
        base_schedule[week] = []
        for matchup in matchups:
            base_schedule[week].append(tuple(matchup))

    labels = config["league"]["labels"][:]

    solutions = []
    seen_schedules = set()

    for perm in itertools.permutations(teams):

        mapping = dict(zip(labels, perm))

        schedule = {}
        bye_assignment = {}

        for w,games in base_schedule.items():

            real_games = []
            playing = set()

            for a,b in games:

                t1 = mapping[a]
                t2 = mapping[b]

                real_games.append((t1,t2))
                playing.add(t1)
                playing.add(t2)

            schedule[w] = real_games

            bye_team = list(set(teams)-playing)[0]
            bye_assignment[bye_team] = w

        if not valid_bye_assignment(bye_assignment):
            continue

        host_options = assign_hosts(schedule)

        if not host_options:
            continue

        score, detail = score_byes(bye_assignment)

        for sched in host_options:

            penalty = host_streak_penalty(sched)

            # canonical schedule signature to remove duplicates
            sig = tuple(
                sorted(
                    (w, tuple(sorted((h,a))))
                    for w in weeks
                    for h,a in sched[w]
                )
            )

            if sig in seen_schedules:
                continue

            seen_schedules.add(sig)

        # higher score is better, lower penalty is better
        solutions.append((score, -penalty, detail, bye_assignment, sched))

    # Let's prioritize by fairness
    solutions.sort(reverse=True, key=lambda x: (x[0], x[1]))

    TOP = 5

    for i,(score,penalty,detail,bye_assignment,schedule) in enumerate(solutions[:TOP]):
        print("\n==============================")
        print("Schedule Rank:", i+1)
        print("Total Score:", score)

        print("\nBye Weeks")

        for t in teams:
            print(f"{t}: week {bye_assignment[t]}  (score {detail[t]})")

        print("\nMatches")

        for w in weeks:

            print(f"\nWeek {w}")

            for host,away in schedule[w]:
                print(f"{host} hosts {away}")

