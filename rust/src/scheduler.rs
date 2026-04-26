use std::collections::{HashMap, HashSet};
use itertools::Itertools;
use rayon::prelude::*;

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

fn score_byes(
    ba: &ByeAssignment,
    prefs: &HashMap<String, [u32; 2]>,
    teams: &[String],
    excluded: &[String],
) -> (i32, HashMap<String, i32>) {
    let mut score = 0;
    let mut detail = HashMap::new();
    for team in teams {
        if excluded.contains(team) {
            detail.insert(team.clone(), -1);
            continue;
        }
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

// ── Structural host assignment ────────────────────────────────────────────────
//
// Works on label indices (0..n_labels) rather than team names. This lets us
// enumerate all valid host-bit patterns once on the base schedule's structure
// and reuse the result across every permutation, instead of re-running the
// full bitmask search inside each permutation iteration.
//
// games_struct: flat list of (week_idx, label_t1, label_t2) sorted for
//               deterministic bit ordering.
// n_teams:      used to enforce the balanced-hosting constraint.
// n_weeks:      length of the sorted week list (weeks must be pre-sorted).

fn structural_host_bits(
    games_struct: &[(usize, usize, usize)],
    n_teams: usize,
    n_weeks: usize,
) -> Vec<u64> {
    let n = games_struct.len();
    let min_hosts = n / n_teams;
    let max_hosts = min_hosts + 1;

    // Per-label bitmasks: which game indices have this label as t1 / t2.
    let mut t1_masks = vec![0u64; n_teams];
    let mut t2_masks = vec![0u64; n_teams];
    for (i, &(_, t1, t2)) in games_struct.iter().enumerate() {
        t1_masks[t1] |= 1 << i;
        t2_masks[t2] |= 1 << i;
    }

    // Per-label, per-week participation masks for the streak check.
    // streak_masks[label][week_idx] = bitmask of game indices in that week.
    let mut streak_masks = vec![vec![0u64; n_weeks]; n_teams];
    for (i, &(wi, t1, t2)) in games_struct.iter().enumerate() {
        streak_masks[t1][wi] |= 1 << i;
        streak_masks[t2][wi] |= 1 << i;
    }

    let mut valid_bits = Vec::new();

    'bits: for bits in 0u64..(1u64 << n) {
        // 1. Balanced-host count check — pure bitmask arithmetic, no allocation.
        for label in 0..n_teams {
            let hosted =
                ((!bits & t1_masks[label]) | (bits & t2_masks[label])).count_ones() as usize;
            if hosted < min_hosts || hosted > max_hosts {
                continue 'bits;
            }
        }

        // 2. Consecutive-hosting streak check — also bitmask, no HashMap needed.
        //    A label hosts in week wi when at least one of its games that week
        //    has it assigned as host (bit=0 → t1 hosts; bit=1 → t2 hosts).
        for label in 0..n_teams {
            let mut streak = 0u32;
            for wi in 0..n_weeks {
                let wm = streak_masks[label][wi];
                let hosts_this_week =
                    ((!bits & t1_masks[label] & wm) | (bits & t2_masks[label] & wm)) != 0;
                if hosts_this_week {
                    streak += 1;
                    if streak >= 3 {
                        continue 'bits;
                    }
                } else {
                    streak = 0;
                }
            }
        }

        valid_bits.push(bits);
    }

    valid_bits
}

fn host_streak_penalty(schedule: &Schedule, weeks: &[u32], teams: &[String]) -> i32 {
    let mut host_weeks: HashMap<&str, Vec<u32>> =
        teams.iter().map(|t| (t.as_str(), vec![])).collect();
    for &w in weeks {
        for (host, _) in &schedule[&w] {
            host_weeks.get_mut(host.as_str()).unwrap().push(w);
        }
    }
    let mut penalty = 0;
    for hw in host_weeks.values_mut() {
        hw.sort_unstable();
        for pair in hw.windows(2) {
            if pair[1] == pair[0] + 1 {
                penalty += 1;
            }
        }
    }
    penalty
}

pub fn run_scheduler(config: &LeagueConfig) -> Result<Vec<Solution>, String> {
    let teams = &config.teams;
    let weeks = &config.weeks;
    let labels = &config.labels;

    if teams.len() < 2 {
        return Err("Need at least 2 teams.".into());
    }
    if weeks.is_empty() {
        return Err("Need at least one week.".into());
    }
    if labels.len() != teams.len() {
        return Err(format!(
            "Labels count ({}) must match team count ({}).",
            labels.len(),
            teams.len()
        ));
    }

    // Stable week ordering so array indices are consistent everywhere.
    let mut sorted_weeks = weeks.clone();
    sorted_weeks.sort_unstable();
    let week_index: HashMap<u32, usize> =
        sorted_weeks.iter().enumerate().map(|(i, &w)| (w, i)).collect();
    let n_weeks = sorted_weeks.len();

    let label_index: HashMap<&str, usize> =
        labels.iter().enumerate().map(|(i, l)| (l.as_str(), i)).collect();

    // Convert base_schedule to structural form: (week_idx, label_t1, label_t2).
    // Also record which label index has the bye each week.
    let mut games_struct: Vec<(usize, usize, usize)> = Vec::new();
    let mut bye_label_per_week: Vec<Option<usize>> = vec![None; n_weeks];

    for (&w, matchups) in &config.base_schedule {
        let wi = week_index[&w];
        let mut playing: HashSet<usize> = HashSet::new();
        for [a, b] in matchups {
            let t1 = label_index[a.as_str()];
            let t2 = label_index[b.as_str()];
            games_struct.push((wi, t1, t2));
            playing.insert(t1);
            playing.insert(t2);
        }
        let sitters: Vec<usize> = (0..labels.len()).filter(|i| !playing.contains(i)).collect();
        if sitters.len() == 1 {
            bye_label_per_week[wi] = Some(sitters[0]);
        }
    }

    // Sort for deterministic bit ordering across calls.
    games_struct.sort_unstable();

    // ── KEY OPTIMISATION ─────────────────────────────────────────────────────
    // Enumerate all valid host-bit patterns once on the structural (label-index)
    // representation. Previously assign_hosts() was called inside the permutation
    // loop, redoing this O(2^n_games) search for every permutation. Now it runs
    // exactly once and the results are shared across all permutation threads.
    let valid_bits = structural_host_bits(&games_struct, teams.len(), n_weeks);

    // Collect permutations (owned Strings) so rayon can distribute them.
    let all_perms: Vec<Vec<String>> = teams
        .iter()
        .permutations(teams.len())
        .map(|p| p.into_iter().cloned().collect())
        .collect();

    // Parallel per-permutation processing.
    // Each thread maps label indices → real team names using the precomputed
    // valid_bits and emits (dedup-signature, Solution) pairs.
    let per_perm_candidates: Vec<Vec<(Vec<(u32, Vec<(String, String)>)>, Solution)>> =
        all_perms.par_iter().map(|perm| {
            // perm[label_idx] = team name for this permutation.
            let mut bye_assignment: ByeAssignment = HashMap::new();
            for (wi, maybe_label) in bye_label_per_week.iter().enumerate() {
                if let Some(&li) = maybe_label.as_ref() {
                    bye_assignment.insert(perm[li].clone(), sorted_weeks[wi]);
                }
            }

            if weeks.len() == 5
                && !valid_bye_assignment(&bye_assignment, &config.bye_restrictions)
            {
                return vec![];
            }

            let (score, detail) = score_byes(
                &bye_assignment,
                &config.bye_preferences,
                teams,
                &config.score_excluded,
            );

            let mut local_candidates = vec![];

            for &bits in &valid_bits {
                // Materialise the named schedule for this (permutation, bits) pair.
                let mut sched: Schedule =
                    sorted_weeks.iter().map(|&w| (w, vec![])).collect();
                for (i, &(wi, t1_li, t2_li)) in games_struct.iter().enumerate() {
                    let w = sorted_weeks[wi];
                    let (host, away) = if (bits >> i) & 1 == 0 {
                        (perm[t1_li].as_str(), perm[t2_li].as_str())
                    } else {
                        (perm[t2_li].as_str(), perm[t1_li].as_str())
                    };
                    sched.get_mut(&w).unwrap().push((host.to_string(), away.to_string()));
                }

                let penalty = host_streak_penalty(&sched, weeks, teams);

                // Canonical deduplication signature (order-independent matchup pairs).
                let mut sig: Vec<(u32, Vec<(String, String)>)> = sorted_weeks
                    .iter()
                    .map(|&w| {
                        let mut gs: Vec<_> = sched[&w]
                            .iter()
                            .map(|(h, a)| {
                                let mut p = [h.clone(), a.clone()];
                                p.sort();
                                (p[0].clone(), p[1].clone())
                            })
                            .collect();
                        gs.sort();
                        (w, gs)
                    })
                    .collect();
                sig.sort_by_key(|&(w, _)| w);

                local_candidates.push((
                    sig,
                    Solution {
                        rank: 0,
                        score,
                        penalty,
                        bye_detail: detail.clone(),
                        bye_assignment: bye_assignment.clone(),
                        schedule: sched,
                    },
                ));
            }
            local_candidates
        })
        .collect();

    // Single-threaded dedup + merge for stable, deterministic ranking.
    let mut seen: HashSet<Vec<(u32, Vec<(String, String)>)>> = HashSet::new();
    let mut solutions: Vec<Solution> = Vec::new();
    for candidates in per_perm_candidates {
        for (sig, sol) in candidates {
            if seen.contains(&sig) {
                continue;
            }
            seen.insert(sig);
            solutions.push(sol);
        }
    }

    solutions.sort_by(|a, b| (b.score, -b.penalty).cmp(&(a.score, -a.penalty)));
    Ok(solutions
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(i, mut s)| {
            s.rank = i + 1;
            s
        })
        .collect())
}
