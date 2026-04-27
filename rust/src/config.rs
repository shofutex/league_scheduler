//! # config.rs
//!
//! Persisted league configuration and supporting helpers.
//!
//! `LeagueConfig` is the single struct that is serialised to JSON on Save and
//! deserialised on Load.  Every field that the wizard collects ends up here.
//!
//! This module also owns:
//! - Two built-in base schedules (`default_5team_schedule`,
//!   `default_6team_schedule`) so new users have a working starting point.
//! - `schedule_to_inputs`: converts a structured schedule back into the
//!   editable string form used by the Step 3 text fields.
//! - `team_color` / `TEAM_COLORS`: a fixed palette used throughout the UI to
//!   colour team names consistently.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use iced::Color;

// ── League configuration ──────────────────────────────────────────────────────

/// All persistent settings for one swim-league season.
///
/// Serialised as JSON for Save/Load.  New fields should carry a
/// `#[serde(default)]` attribute so that older saved files remain loadable.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueConfig {
    /// Ordered list of real team names (e.g. `["Marlins", "Dolphins", …]`).
    /// The scheduler maps these onto the abstract label slots (A, B, C…) by
    /// trying every permutation.
    pub teams: Vec<String>,

    /// The calendar week numbers included in this season (e.g. `[1,2,3,4,5]`).
    /// May be non-contiguous (e.g. `[1,2,4,5]` if week 3 is a holiday).
    pub weeks: Vec<u32>,

    /// Abstract label identifiers, one per team, in team order.
    /// Always a prefix of `['A','B','C','D','E','F','G','H']`.
    /// `labels[i]` is the label assigned to `teams[i]`.
    ///
    /// Kept in sync with `teams` via `SwimScheduler::sync_labels()`.
    pub labels: Vec<String>,

    /// The base schedule: maps each week number to a list of label matchups.
    ///
    /// Each matchup is `[label_a, label_b]` where `label_a` and `label_b` are
    /// elements of `labels`.  The team not mentioned in any matchup for a given
    /// week has the bye that week.
    ///
    /// Example (5-team, week 1): `{ 1: [["A","B"], ["C","D"]] }` means label E
    /// has the bye in week 1.
    pub base_schedule: HashMap<u32, Vec<[String; 2]>>,

    /// Per-team bye-week preferences.
    ///
    /// `bye_preferences["Marlins"] = [3, 5]` means the Marlins would prefer
    /// their bye in week 3 (1st choice, worth 2 points) or week 5 (2nd choice,
    /// worth 1 point).  Teams without an entry are treated as having no
    /// preference (0 points regardless of which week they receive).
    pub bye_preferences: HashMap<String, [u32; 2]>,

    /// Per-team hard bye-week restrictions.
    ///
    /// `bye_restrictions["Dolphins"] = [1, 2]` means the Dolphins must NOT
    /// have their bye in week 1 or week 2.  Any schedule that would assign
    /// such a bye is discarded before scoring.
    pub bye_restrictions: HashMap<String, Vec<u32>>,

    /// Teams excluded from the fairness-score calculation.
    ///
    /// Excluded teams still receive byes and appear in the schedule normally.
    /// They simply contribute 0 to the score used to rank solutions, so they
    /// do not skew rankings when their bye preference is irrelevant (e.g. the
    /// team captain doesn't mind which week they sit out).
    #[serde(default)] // older saved files won't have this field
    pub score_excluded: Vec<String>,
}

impl Default for LeagueConfig {
    /// Produce a ready-to-use 5-team default config so new users can click
    /// straight through the wizard without entering anything.
    fn default() -> Self {
        Self {
            teams: vec![],
            weeks: vec![1, 2, 3, 4, 5],
            labels: vec!["A", "B", "C", "D", "E"]
                .into_iter()
                .map(String::from)
                .collect(),
            base_schedule: default_5team_schedule(),
            bye_preferences: HashMap::new(),
            bye_restrictions: HashMap::new(),
            score_excluded: vec![],
        }
    }
}

// ── Built-in base schedules ───────────────────────────────────────────────────

/// Standard 5-team round-robin base schedule.
///
/// Five weeks, four games total (two per week except the week of the bye).
/// Each label plays every other label exactly once over the season.
/// Each label sits out exactly one week (their bye).
///
/// Week structure:
/// ```text
/// Week 1: A–B, C–D   (E has bye)
/// Week 2: A–C, D–E   (B has bye)
/// Week 3: A–D, B–E   (C has bye)
/// Week 4: A–E, B–C   (D has bye)
/// Week 5: B–D, C–E   (A has bye)
/// ```
pub fn default_5team_schedule() -> HashMap<u32, Vec<[String; 2]>> {
    let mut m = HashMap::new();
    m.insert(1, vec![["A", "B"], ["C", "D"]]);
    m.insert(2, vec![["A", "C"], ["D", "E"]]);
    m.insert(3, vec![["A", "D"], ["B", "E"]]);
    m.insert(4, vec![["A", "E"], ["B", "C"]]);
    m.insert(5, vec![["B", "D"], ["C", "E"]]);
    // Convert `&str` pairs to owned `String` pairs.
    m.into_iter()
        .map(|(k, v)| {
            (
                k,
                v.into_iter()
                    .map(|[a, b]| [a.into(), b.into()])
                    .collect(),
            )
        })
        .collect()
}

/// Standard 6-team round-robin base schedule.
///
/// Five weeks, three games per week.  With six teams there are no byes —
/// every team plays every week.  Each label plays every other label exactly
/// once over the five weeks.
///
/// Week structure:
/// ```text
/// Week 1: A–B, C–F, D–E
/// Week 2: A–C, B–E, D–F
/// Week 3: A–D, B–F, C–E
/// Week 4: A–E, B–C, D–F
/// Week 5: A–F, B–D, C–E
/// ```
pub fn default_6team_schedule() -> HashMap<u32, Vec<[String; 2]>> {
    let mut m: HashMap<u32, Vec<[&str; 2]>> = HashMap::new();
    m.insert(1, vec![["A", "B"], ["C", "F"], ["D", "E"]]);
    m.insert(2, vec![["A", "C"], ["B", "E"], ["D", "F"]]);
    m.insert(3, vec![["A", "D"], ["B", "F"], ["C", "E"]]);
    m.insert(4, vec![["A", "E"], ["B", "C"], ["D", "F"]]);
    m.insert(5, vec![["A", "F"], ["B", "D"], ["C", "E"]]);
    m.into_iter()
        .map(|(k, v)| {
            (
                k,
                v.into_iter()
                    .map(|[a, b]| [a.into(), b.into()])
                    .collect(),
            )
        })
        .collect()
}

// ── UI helpers ────────────────────────────────────────────────────────────────

/// Convert a structured base schedule into per-week editable strings.
///
/// Each week's matchup list is formatted as `"A vs B, C vs D"` so it can be
/// pre-populated into the Step 3 text fields.  Called:
/// - At startup to initialise fields from the default config.
/// - After `LoadConfig` to reflect the loaded schedule.
/// - After `ApplyWeeks` to add/remove rows when the week set changes.
/// - After `UseDefault5Team` / `UseDefault6Team`.
pub fn schedule_to_inputs(bs: &HashMap<u32, Vec<[String; 2]>>) -> HashMap<u32, String> {
    bs.iter()
        .map(|(&w, games)| {
            let line = games
                .iter()
                .map(|[a, b]| format!("{} vs {}", a, b))
                .collect::<Vec<_>>()
                .join(", ");
            (w, line)
        })
        .collect()
}

// ── Team colour palette ───────────────────────────────────────────────────────

/// Fixed palette of visually distinct colours used to identify teams in the UI.
///
/// Colours are chosen to be readable against the Tokyo Night dark background
/// and to remain distinguishable even under common forms of colour-blindness.
/// The palette cycles if there are more teams than colours (unlikely in
/// practice, since the scheduler supports at most 6 teams).
const TEAM_COLORS: &[Color] = &[
    Color { r: 0.37, g: 0.70, b: 0.96, a: 1.0 }, // blue
    Color { r: 0.55, g: 0.91, b: 0.64, a: 1.0 }, // green
    Color { r: 1.00, g: 0.72, b: 0.42, a: 1.0 }, // orange
    Color { r: 0.90, g: 0.50, b: 0.75, a: 1.0 }, // pink
    Color { r: 0.70, g: 0.60, b: 0.95, a: 1.0 }, // purple
    Color { r: 0.45, g: 0.88, b: 0.85, a: 1.0 }, // teal
    Color { r: 0.95, g: 0.55, b: 0.55, a: 1.0 }, // red
    Color { r: 0.95, g: 0.90, b: 0.45, a: 1.0 }, // yellow
];

/// Look up the display colour assigned to `team` within `teams`.
///
/// The colour is determined by the team's position in the `teams` slice so
/// that the same team always gets the same colour as long as the team list
/// order is unchanged.  Falls back to the first colour if the team is not
/// found (should not happen in normal usage).
pub fn team_color(teams: &[String], team: &str) -> Color {
    let idx = teams.iter().position(|t| t == team).unwrap_or(0);
    TEAM_COLORS[idx % TEAM_COLORS.len()]
}
