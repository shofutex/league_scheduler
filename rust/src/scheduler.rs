//! # scheduler.rs
//!
//! Core combinatorial scheduling engine for the Swim League Scheduler.
//!
//! ## High-level algorithm
//!
//! The goal is to assign real team names to the abstract label slots (A, B, C…)
//! defined in the base schedule, and to decide which team hosts each match,
//! such that:
//!
//! 1. **Bye preferences** are satisfied as much as possible (scored 0–2 per team).
//! 2. **Bye restrictions** are never violated (hard constraint).
//! 3. **Hosting is balanced** — every team hosts roughly the same number of games.
//! 4. **No team hosts three or more consecutive weeks** (soft penalty).
//!
//! ### Steps
//!
//! 1. Convert `config.base_schedule` into a structural representation using
//!    label *indices* rather than names.  This separates "which pairs play each
//!    week" from "which real team maps to which label".
//!
//! 2. **Pre-compute valid host-bit patterns** once on the structural form
//!    (`structural_host_bits`).  Each game gets one bit: 0 = first label hosts,
//!    1 = second label hosts.  We enumerate all 2^n_games patterns and keep
//!    only those that satisfy the balance and streak constraints.  This is done
//!    *once* and the results are shared across all permutations.
//!
//! 3. **Enumerate team permutations** in parallel (rayon).  Each permutation
//!    is a mapping `label_index → team_name`.  For every permutation we check
//!    the bye-restriction hard constraint, compute the fairness score, then
//!    combine with every valid host-bit pattern to produce a concrete named
//!    schedule.
//!
//! 4. **Deduplicate** schedules that are structurally identical (same unordered
//!    matchup pairs each week, regardless of which permutation produced them).
//!
//! 5. **Rank** the remaining unique solutions by (score DESC, penalty ASC) and
//!    return the top 5.

use std::collections::{HashMap, HashSet};
use itertools::Itertools;
use rayon::prelude::*;

use crate::config::LeagueConfig;

// ── Type aliases ──────────────────────────────────────────────────────────────

/// A single game: (host_team_name, away_team_name).
/// The host is always listed first.
type Matchup = (String, String);

/// A full season schedule: week_number → list of games that week.
type Schedule = HashMap<u32, Vec<Matchup>>;

/// Maps each team name to the week number it has a bye.
type ByeAssignment = HashMap<String, u32>;

// ── Solution ──────────────────────────────────────────────────────────────────

/// One complete, valid schedule solution returned by the scheduler.
#[derive(Debug, Clone)]
pub struct Solution {
    /// 1-based rank (1 = best score, up to 5).
    pub rank: usize,

    /// Total fairness score across all teams.
    /// Each team earns 2 points for getting its 1st-choice bye week,
    /// 1 point for its 2nd choice, and 0 for neither.
    /// Excluded teams contribute 0.
    pub score: i32,

    /// Soft penalty for consecutive hosting streaks.
    /// Incremented once for every adjacent pair of weeks where the same
    /// team hosts.  Lower is better.
    pub penalty: i32,

    /// Per-team bye score breakdown.
    /// Values: 2 = 1st pref, 1 = 2nd pref, 0 = no pref match, -1 = excluded.
    pub bye_detail: HashMap<String, i32>,

    /// The week each team has a bye.
    pub bye_assignment: ByeAssignment,

    /// The full named schedule (host, away) pairs per week.
    pub schedule: Schedule,
}

// ── Constraint helpers ────────────────────────────────────────────────────────

/// Returns `true` if no team in `ba` is assigned a bye week that appears in
/// its restriction list.
///
/// This is a **hard constraint** — any bye assignment that fails this check
/// is discarded entirely before any host-bit patterns are tried.
fn valid_bye_assignment(ba: &ByeAssignment, restrictions: &HashMap<String, Vec<u32>>) -> bool {
    for (team, week) in ba {
        if restrictions.get(team).map_or(false, |r| r.contains(week)) {
            return false;
        }
    }
    true
}

/// Compute the total fairness score and per-team breakdown for a bye assignment.
///
/// Scoring rules:
/// - Team is in `excluded` → score contribution −1 (sentinel, not counted).
/// - Bye week matches team's 1st preference → +2.
/// - Bye week matches team's 2nd preference → +1.
/// - No preference set, or neither preference matches → +0.
///
/// Returns `(total_score, per_team_detail_map)`.
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
            // Store -1 so the view can display the "excluded" indicator.
            detail.insert(team.clone(), -1);
            continue;
        }
        if let Some(&week) = ba.get(team) {
            let s = match prefs.get(team) {
                Some(&[f, _]) if week == f => 2, // 1st preference
                Some(&[_, s]) if week == s => 1, // 2nd preference
                _                          => 0, // no match
            };
            score += s;
            detail.insert(team.clone(), s);
        }
    }
    (score, detail)
}

// ── Host-bit enumeration ──────────────────────────────────────────────────────

/// Enumerate all valid host-assignment bit patterns for a given structural
/// schedule layout, returning only those that satisfy the balance and
/// no-long-streak constraints.
///
/// ## Representation
///
/// `games_struct` is a sorted flat list of `(week_idx, label_t1, label_t2)`
/// tuples.  Each entry corresponds to one game.  We assign one bit per game:
///
/// - `bit = 0` → the team mapped to `label_t1` hosts.
/// - `bit = 1` → the team mapped to `label_t2` hosts.
///
/// So a u64 with `n` games set encodes a full host assignment for that week.
///
/// ## Constraints checked
///
/// 1. **Balance** — each label must host between `floor(n_games / n_teams)` and
///    `floor(n_games / n_teams) + 1` games over the whole season.
///
/// 2. **No streak of 3+** — no label may host in three or more consecutive weeks.
///    Two consecutive hosting weeks is allowed (penalty is counted separately);
///    three or more is rejected outright.
///
/// ## Why this is done once
///
/// The structural representation uses label *indices*, not team names.  Because
/// the constraints depend only on the game structure (which labels play which
/// weeks), the set of valid bit patterns is the same for every team permutation.
/// Computing it once and sharing it avoids repeating O(2^n_games) work inside
/// the inner permutation loop.
fn structural_host_bits(
    games_struct: &[(usize, usize, usize)], // (week_idx, t1_label_idx, t2_label_idx)
    n_teams: usize,
    n_weeks: usize,
) -> Vec<u64> {
    let n = games_struct.len(); // total number of games across the season

    // Each team should host approximately n_games / n_teams times.
    // We allow ±1 so uneven totals are accommodated.
    let min_hosts = n / n_teams;
    let max_hosts = min_hosts + 1;

    // Pre-compute bitmasks so constraint checks are pure integer arithmetic.

    // t1_masks[label]: bitmask of game indices where this label is listed as t1.
    // t2_masks[label]: bitmask of game indices where this label is listed as t2.
    // A label "hosts" game i when:
    //   - it is t1 and bit i = 0  (t1 hosts), OR
    //   - it is t2 and bit i = 1  (t2 hosts).
    // Hosted count = popcount( (~bits & t1_mask) | (bits & t2_mask) ).
    let mut t1_masks = vec![0u64; n_teams];
    let mut t2_masks = vec![0u64; n_teams];
    for (i, &(_, t1, t2)) in games_struct.iter().enumerate() {
        t1_masks[t1] |= 1 << i;
        t2_masks[t2] |= 1 << i;
    }

    // streak_masks[label][week_idx]: bitmask of game indices that involve this
    // label in this week.  Used to test whether a label hosts any game in a
    // given week (for the streak check).
    let mut streak_masks = vec![vec![0u64; n_weeks]; n_teams];
    for (i, &(wi, t1, t2)) in games_struct.iter().enumerate() {
        streak_masks[t1][wi] |= 1 << i;
        streak_masks[t2][wi] |= 1 << i;
    }

    let mut valid_bits = Vec::new();

    'bits: for bits in 0u64..(1u64 << n) {
        // ── Constraint 1: balanced host counts ───────────────────────────────
        for label in 0..n_teams {
            // Count how many games this label hosts under the current bit pattern.
            let hosted =
                ((!bits & t1_masks[label]) | (bits & t2_masks[label])).count_ones() as usize;
            if hosted < min_hosts || hosted > max_hosts {
                continue 'bits; // reject this pattern
            }
        }

        // ── Constraint 2: no 3+-week hosting streak ──────────────────────────
        // Walk weeks in order; count consecutive weeks where the label hosts
        // at least one game.  Reset the counter when the label doesn't host.
        for label in 0..n_teams {
            let mut streak = 0u32;
            for wi in 0..n_weeks {
                // Games involving this label in this week.
                let wm = streak_masks[label][wi];
                // Does this label host any game this week under `bits`?
                let hosts_this_week =
                    ((!bits & t1_masks[label] & wm) | (bits & t2_masks[label] & wm)) != 0;
                if hosts_this_week {
                    streak += 1;
                    if streak >= 3 {
                        continue 'bits; // three consecutive weeks — reject
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

// ── Soft penalty ──────────────────────────────────────────────────────────────

/// Compute the hosting-streak soft penalty for a fully materialised named schedule.
///
/// For every pair of adjacent weeks where the same team hosts at least one game
/// in both, the penalty is incremented by 1.  A lower penalty is better.
///
/// This is a *soft* penalty (counted but not filtered) so that schedules with
/// mild streaks can still appear in the ranked results if their fairness score
/// is high enough.
fn host_streak_penalty(schedule: &Schedule, weeks: &[u32], teams: &[String]) -> i32 {
    // Collect the set of weeks each team hosts at least one game.
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
        // sliding window of size 2: adjacent weeks that are exactly 1 apart.
        for pair in hw.windows(2) {
            if pair[1] == pair[0] + 1 {
                penalty += 1;
            }
        }
    }
    penalty
}

// ── Main entry point ──────────────────────────────────────────────────────────

/// Run the scheduler and report progress to a callback as work proceeds.
///
/// # Progress reporting
///
/// `progress_cb` is called with values in `[0.0, 1.0]`:
/// - `0.05` — validation passed, structural pre-computation starting.
/// - `0.15` — host-bit enumeration complete.
/// - `0.15` → `0.97` — one tick per permutation processed (rayon parallel).
/// - `0.97` — parallel phase done, dedup and ranking starting.
/// - `1.0`  — complete.
///
/// # Thread-safety of the callback
///
/// The callback is called from rayon worker threads.  It is wrapped in an
/// `Arc<Mutex<F>>` internally so `F` only needs to be `FnMut + Send`.
///
/// # Returns
///
/// Up to 5 unique `Solution` values sorted by (score DESC, penalty ASC),
/// or an `Err` if validation fails.
pub fn run_scheduler_with_progress<F>(
    config: &LeagueConfig,
    progress_cb: F,
) -> Result<Vec<Solution>, String>
where
    F: FnMut(f32) + Send,
{
    let teams  = &config.teams;
    let weeks  = &config.weeks;
    let labels = &config.labels;

    // ── Input validation ──────────────────────────────────────────────────────

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

    // ── Stable week ordering ──────────────────────────────────────────────────
    // Sort weeks so array indices (week_index) are consistent everywhere.
    // The user may have entered weeks out of order; we normalise here.
    let mut sorted_weeks = weeks.clone();
    sorted_weeks.sort_unstable();
    let week_index: HashMap<u32, usize> =
        sorted_weeks.iter().enumerate().map(|(i, &w)| (w, i)).collect();
    let n_weeks = sorted_weeks.len();

    // Map label strings (e.g. "A", "B") to 0-based indices for the bitmask math.
    let label_index: HashMap<&str, usize> =
        labels.iter().enumerate().map(|(i, l)| (l.as_str(), i)).collect();

    // ── Build structural game list ────────────────────────────────────────────
    // Convert config.base_schedule (which uses label strings) into a flat list
    // of (week_idx, t1_label_idx, t2_label_idx) triples.
    // Simultaneously detect which label has the bye each week.
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
        // The one label not in any matchup this week has the bye.
        let sitters: Vec<usize> = (0..labels.len()).filter(|i| !playing.contains(i)).collect();
        if sitters.len() == 1 {
            bye_label_per_week[wi] = Some(sitters[0]);
        }
    }

    // Sort for deterministic bit ordering — every call to this function must
    // produce the same games_struct ordering so that bit indices are stable.
    games_struct.sort_unstable();

    // ── Wrap the callback for thread-safe multi-threaded access ───────────────
    // `FnMut` is not `Sync`, so we can't share a reference to it across rayon
    // threads.  Wrapping in Arc<Mutex<F>> makes it safely callable from any
    // thread while only one thread calls at a time.
    let progress_cb = std::sync::Arc::new(std::sync::Mutex::new(progress_cb));

    // Convenience closure for the non-rayon call sites (before/after par_iter).
    // Inside rayon closures we lock progress_cb directly because `call_progress`
    // itself captures `progress_cb` by reference and is not `Send`.
    let call_progress = |p: f32| {
        if let Ok(mut cb) = progress_cb.lock() { cb(p); }
    };

    // ── Phase 1: structural host-bit enumeration (single-threaded) ────────────
    call_progress(0.05);
    let valid_bits = structural_host_bits(&games_struct, teams.len(), n_weeks);
    call_progress(0.15);

    // ── Phase 2: permutation enumeration (parallel) ───────────────────────────
    // Collect all n! orderings of team names up-front so rayon can slice the
    // workload evenly.  Each permutation is a Vec<String> mapping
    // `label_index → team_name`.
    let all_perms: Vec<Vec<String>> = teams
        .iter()
        .permutations(teams.len())
        .map(|p| p.into_iter().cloned().collect())
        .collect();

    let total_perms = all_perms.len();

    // Atomic counter shared across rayon worker threads so each thread can
    // increment "how many permutations are done" without a mutex on the hot path.
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    let completed       = Arc::new(AtomicUsize::new(0));
    let completed_clone = Arc::clone(&completed);

    // The outer Vec has one entry per permutation; each entry is a list of
    // (dedup_signature, Solution) candidates produced by that permutation.
    // We flatten and deduplicate after the parallel phase.
    let per_perm_candidates: Vec<Vec<(Vec<(u32, Vec<(String, String)>)>, Solution)>> =
        all_perms.par_iter().map(|perm| {
            // ── Derive bye assignment for this permutation ────────────────────
            // For each week that has a bye label, look up which real team name
            // that label maps to in this permutation.
            let mut bye_assignment: ByeAssignment = HashMap::new();
            for (wi, maybe_label) in bye_label_per_week.iter().enumerate() {
                if let Some(&li) = maybe_label.as_ref() {
                    bye_assignment.insert(perm[li].clone(), sorted_weeks[wi]);
                }
            }

            // ── Hard constraint: bye restrictions ────────────────────────────
            // For 5-team schedules the bye assignment is fully determined by the
            // permutation, so we can reject here before trying any host patterns.
            // (6-team schedules have no byes so this branch is skipped.)
            if weeks.len() == 5
                && !valid_bye_assignment(&bye_assignment, &config.bye_restrictions)
            {
                // Still tick progress so the bar advances correctly.
                let done = completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
                if let Ok(mut cb) = progress_cb.lock() {
                    cb(0.15 + 0.80 * (done as f32 / total_perms as f32));
                }
                return vec![]; // discard this permutation
            }

            // ── Fairness score ────────────────────────────────────────────────
            // Computed once per permutation — it depends only on the bye
            // assignment, not on the host-bit pattern.
            let (score, detail) = score_byes(
                &bye_assignment,
                &config.bye_preferences,
                teams,
                &config.score_excluded,
            );

            let mut local_candidates = vec![];

            // ── Try every valid host-bit pattern ─────────────────────────────
            for &bits in &valid_bits {
                // Materialise the named schedule: for each game, use the bit to
                // decide which label hosts, then look up that label's team name
                // in the current permutation.
                let mut sched: Schedule =
                    sorted_weeks.iter().map(|&w| (w, vec![])).collect();
                for (i, &(wi, t1_li, t2_li)) in games_struct.iter().enumerate() {
                    let w = sorted_weeks[wi];
                    let (host, away) = if (bits >> i) & 1 == 0 {
                        // bit 0 → t1 hosts
                        (perm[t1_li].as_str(), perm[t2_li].as_str())
                    } else {
                        // bit 1 → t2 hosts
                        (perm[t2_li].as_str(), perm[t1_li].as_str())
                    };
                    sched.get_mut(&w).unwrap().push((host.to_string(), away.to_string()));
                }

                let penalty = host_streak_penalty(&sched, weeks, teams);

                // ── Deduplication signature ───────────────────────────────────
                // Two schedules are considered identical if they have the same
                // unordered set of matchup pairs each week (regardless of which
                // team is listed as host — host assignment is already baked in,
                // so we sort each pair alphabetically for canonical form).
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
                        rank: 0, // assigned after dedup + sort
                        score,
                        penalty,
                        bye_detail: detail.clone(),
                        bye_assignment: bye_assignment.clone(),
                        schedule: sched,
                    },
                ));
            }

            // Advance the progress bar for this permutation.
            let done = completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
            if let Ok(mut cb) = progress_cb.lock() {
                cb(0.15 + 0.80 * (done as f32 / total_perms as f32));
            }

            local_candidates
        })
        .collect();

    call_progress(0.97);

    // ── Phase 3: deduplicate and rank (single-threaded) ───────────────────────
    // Merge all per-permutation candidate lists, discarding any schedule whose
    // canonical signature we have already seen.
    let mut seen: HashSet<Vec<(u32, Vec<(String, String)>)>> = HashSet::new();
    let mut solutions: Vec<Solution> = Vec::new();
    for candidates in per_perm_candidates {
        for (sig, sol) in candidates {
            if seen.contains(&sig) {
                continue; // duplicate — same matchup pairs, skip
            }
            seen.insert(sig);
            solutions.push(sol);
        }
    }

    // Sort: higher score first; within equal score, lower penalty first.
    // The tuple comparison `(b.score, -b.penalty).cmp(...)` achieves this
    // because negating the penalty reverses its sort direction.
    solutions.sort_by(|a, b| (b.score, -b.penalty).cmp(&(a.score, -a.penalty)));

    // Keep only the top 5 and assign 1-based rank numbers.
    let result = Ok(solutions
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(i, mut s)| { s.rank = i + 1; s })
        .collect());

    call_progress(1.0);
    result
}

/// Convenience wrapper that runs the scheduler with no progress reporting.
///
/// Kept for symmetry and potential future use (e.g. headless testing).
/// Delegates entirely to [`run_scheduler_with_progress`].
pub fn run_scheduler(config: &LeagueConfig) -> Result<Vec<Solution>, String> {
    run_scheduler_with_progress(config, |_| {})
}
