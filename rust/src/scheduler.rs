use std::collections::{HashMap, HashSet};
use itertools::Itertools;


use crate::config::LeagueConfig;


// ── Scheduling ────────────────────────────────────────────────────────────────

type Matchup = (String, String);
type Schedule = HashMap<u32, Vec<Matchup>>;
type ByeAssignment = HashMap<String, u32>;

#[derive(Debug, Clone)]
pub struct Solution {
    pub rank: usize,
    pub score: i32,
    pub penalty: i32,
    pub bye_detail: HashMap<String, i32>,
    pub bye_assignment: ByeAssignment,
    pub schedule: Schedule,
}

fn valid_bye_assignment(ba: &ByeAssignment, restrictions: &HashMap<String, Vec<u32>>) -> bool {
    for (team, week) in ba {
        if restrictions.get(team).map_or(false, |r| r.contains(week)) {
            return false;
        }
    }
    true
}

fn score_byes(ba: &ByeAssignment, prefs: &HashMap<String, [u32;2]>, teams: &[String], excluded: &[String]) -> (i32, HashMap<String,i32>) {
    let mut score = 0;
    let mut detail = HashMap::new();
    for team in teams {
        if excluded.contains(team) {
            detail.insert(team.clone(), -1); // sentinel: excluded
            continue;
        }
        
        // A 6-team league doesn't get bye weeks, so ba may be empty
        if let Some(&week) = ba.get(team) {
            let s = match prefs.get(team) {
                Some(&[f, _]) if week == f => 2,
                Some(&[_, s]) if week == s => 1,
                _ => 0,
            };
            score += s;
            detail.insert(team.clone(), s);
        }
    }
    (score, detail)
}

fn assign_hosts(schedule: &Schedule, weeks: &[u32], teams: &[String]) -> Vec<Schedule> {
    let games: Vec<(u32, Matchup)> = weeks.iter()
        .flat_map(|&w| schedule[&w].iter().map(move |(a,b)| (w,(a.clone(),b.clone()))))
        .collect();
    let n = games.len();
    let min_hosts = n / teams.len();
    let max_hosts = min_hosts + 1;

    // For each team, precompute a bitmask of which game indices have them as t1 or t2
    let team_masks: Vec<(u64, u64)> = teams.iter().map(|t| {
        let t1_mask = games.iter().enumerate()
            .filter(|(_, (_, (a, _)))| a == t)
            .fold(0u64, |acc, (i, _)| acc | (1 << i));
        let t2_mask = games.iter().enumerate()
            .filter(|(_, (_, (_, b)))| b == t)
            .fold(0u64, |acc, (i, _)| acc | (1 << i));
        (t1_mask, t2_mask)
    }).collect();

    let mut options = Vec::new();
    'bits: for bits in 0u64..(1u64 << n) {
        // Check counts via bitmask before building anything
        for (t1_mask, t2_mask) in &team_masks {
            // team hosts when it's t1 and bit=0, or t2 and bit=1
            let hosted = ((!bits & t1_mask) | (bits & t2_mask)).count_ones() as usize;
            if hosted < min_hosts || hosted > max_hosts { continue 'bits; }
        }

        // Build schedule only for bitmasks that pass the count check
        let mut sched: Schedule = weeks.iter().map(|&w| (w, vec![])).collect();
        for (i, (w, (t1, t2))) in games.iter().enumerate() {
            let (host, away) = if (bits >> i) & 1 == 0 { (t1.as_str(), t2.as_str()) } else { (t2.as_str(), t1.as_str()) };
            sched.get_mut(w).unwrap().push((host.to_string(), away.to_string()));
        }

        // Reject any assignment where a team hosts 3+ consecutive weeks
        let no_long_streak = {
            let mut ok = true;
            'outer: for team in teams {
                let mut host_weeks: Vec<u32> = weeks.iter()
                    .filter(|&&w| sched[&w].iter().any(|(h, _)| h == team))
                    .copied()
                    .collect();
                host_weeks.sort_unstable();
                let mut streak = 1u32;
                for pair in host_weeks.windows(2) {
                    if pair[1] == pair[0] + 1 {
                        streak += 1;
                        if streak >= 3 {
                            ok = false;
                            break 'outer;
                        }
                    } else {
                        streak = 1;
                    }
                }
            }
            ok
        };
        if no_long_streak { options.push(sched); }
    }
    options
}

fn host_streak_penalty(schedule: &Schedule, weeks: &[u32], teams: &[String]) -> i32 {
    let mut host_weeks: HashMap<&str,Vec<u32>> = teams.iter().map(|t|(t.as_str(),vec![])).collect();
    for &w in weeks {
        for (host,_) in &schedule[&w] {
            host_weeks.get_mut(host.as_str()).unwrap().push(w);
        }
    }
    let mut penalty = 0;
    for hw in host_weeks.values_mut() {
        hw.sort_unstable();
        for pair in hw.windows(2) { if pair[1]==pair[0]+1 { penalty+=1; } }
    }
    penalty
}

pub fn run_scheduler(config: &LeagueConfig) -> Result<Vec<Solution>, String> {
    let teams = &config.teams;
    let weeks = &config.weeks;
    let labels = &config.labels;

    if teams.len() < 2 { return Err("Need at least 2 teams.".into()); }
    if weeks.is_empty() { return Err("Need at least one week.".into()); }
    if labels.len() != teams.len() {
        return Err(format!("Labels count ({}) must match team count ({}).", labels.len(), teams.len()));
    }
    let mut solutions: Vec<Solution> = Vec::new();
    let mut seen: HashSet<Vec<(u32,Vec<(String,String)>)>> = HashSet::new();
    let team_set: HashSet<&str> = teams.iter().map(|s|s.as_str()).collect();
    
    for perm in teams.iter().permutations(teams.len()) {        
        let mapping: HashMap<&str,&str> = labels.iter().zip(perm.iter()).map(|(l,t)|(l.as_str(),t.as_str())).collect();
        let mut schedule: Schedule = HashMap::new();
        let mut bye_assignment: ByeAssignment = HashMap::new();

        for (&w, games) in &config.base_schedule {
            let mut real_games = vec![];
            let mut playing: HashSet<&str> = HashSet::new();
            for [a,b] in games {
                let t1 = mapping[a.as_str()];
                let t2 = mapping[b.as_str()];
                real_games.push((t1.to_string(),t2.to_string()));
                playing.insert(t1); playing.insert(t2);
            }
            schedule.insert(w, real_games);
            let sitters: Vec<_> = team_set.difference(&playing).copied().collect();

            if sitters.len()==1 { bye_assignment.insert(sitters[0].to_string(), w); }
        }

        if weeks.len() == 5 && !valid_bye_assignment(&bye_assignment, &config.bye_restrictions) { continue; }
        let host_options = assign_hosts(&schedule, weeks, teams);
        if host_options.is_empty() { eprintln!("Empty hosts"); continue; }
        let (score, detail) = score_byes(&bye_assignment, &config.bye_preferences, teams, &config.score_excluded);

        for sched in host_options {
            let penalty = host_streak_penalty(&sched, weeks, teams);
            let mut sig: Vec<(u32,Vec<(String,String)>)> = weeks.iter().map(|&w| {
                let mut gs: Vec<_> = sched[&w].iter().map(|(h,a)| {
                    let mut p = [h.clone(),a.clone()]; p.sort(); (p[0].clone(),p[1].clone())
                }).collect();
                gs.sort(); (w,gs)
            }).collect();
            sig.sort_by_key(|(w,_)| *w);
            if seen.contains(&sig) { continue; }
            seen.insert(sig);
            solutions.push(Solution { rank:0, score, penalty, bye_detail:detail.clone(), bye_assignment:bye_assignment.clone(), schedule:sched });
        }
    }

    solutions.sort_by(|a,b| (b.score,-b.penalty).cmp(&(a.score,-a.penalty)));
    Ok(solutions.into_iter().take(5).enumerate().map(|(i,mut s)| { s.rank=i+1; s }).collect())
}

