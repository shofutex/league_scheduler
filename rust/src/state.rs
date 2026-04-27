//! # state.rs
//!
//! Central application state for the Swim League Scheduler.
//!
//! `SwimScheduler` is the single source of truth that iced passes to both
//! `update()` and `view()` on every event cycle.  All fields are `pub` so
//! that `update.rs` and `view.rs` can read them directly without needing
//! accessor boilerplate — iced's architecture makes this safe because only
//! `update()` ever mutates the struct.
//!
//! `Step` models the wizard's linear progression.  Steps are visited in
//! declaration order; `Step::all()` returns that canonical slice so nav
//! logic never has to hard-code indices.

use crate::config::LeagueConfig;
use crate::config::schedule_to_inputs;
use crate::scheduler::Solution;
use crate::message::Message;
use std::collections::HashMap;
use iced::Task;

// ── Wizard step enum ──────────────────────────────────────────────────────────

/// One screen in the multi-step wizard.
///
/// The variants are listed in the order the user visits them.  `Step::all()`
/// returns a `&'static [Step]` of all variants in that order so that the
/// navigation helpers in `update.rs` can compute "next" and "prev" without
/// duplicating the sequence.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Step 1 — add / remove team names.
    Teams,
    /// Step 2 — define which calendar weeks are in the season.
    Weeks,
    /// Step 3 — enter or load the label-based base matchup schedule.
    BaseSchedule,
    /// Step 4 — optionally set each team's 1st- and 2nd-choice bye week.
    ByePreferences,
    /// Step 5 — optionally forbid certain bye weeks for certain teams.
    ByeRestrictions,
    /// Step 6 — optionally exclude teams from influencing the fairness score.
    ScoreExclusions,
    /// Final screen — ranked schedule solutions with export option.
    Results,
}

impl Step {
    /// Short title shown in the app header beneath the app name.
    pub fn title(&self) -> &'static str {
        match self {
            Step::Teams          => "Step 1 — Teams",
            Step::Weeks          => "Step 2 — Weeks",
            Step::BaseSchedule   => "Step 3 — Base Schedule",
            Step::ByePreferences => "Step 4 — Bye Preferences",
            Step::ByeRestrictions => "Step 5 — Bye Restrictions",
            Step::ScoreExclusions => "Step 6 — Score Exclusions",
            Step::Results        => "Results",
        }
    }

    /// The canonical ordered sequence of all steps.
    ///
    /// Navigation code indexes into this slice rather than hard-coding
    /// numbers, so adding a new step only requires inserting it here and
    /// in the `match` above.
    pub fn all() -> &'static [Step] {
        &[
            Step::Teams,
            Step::Weeks,
            Step::BaseSchedule,
            Step::ByePreferences,
            Step::ByeRestrictions,
            Step::ScoreExclusions,
            Step::Results,
        ]
    }
}

// ── Application state ─────────────────────────────────────────────────────────

/// The complete runtime state of the Swim League Scheduler application.
///
/// iced calls `view(&self)` to build the widget tree and `update(&mut self,
/// msg)` to apply each incoming `Message`.  Because `update` is the only
/// mutation point, there are no races or hidden side-effects.
#[derive(Debug, Default)]
pub struct SwimScheduler {
    // ── Persisted configuration ───────────────────────────────────────────────

    /// The authoritative league configuration: teams, weeks, base schedule,
    /// preferences, restrictions, and score-excluded teams.
    /// This is what gets serialised to JSON on Save and deserialised on Load.
    pub config: LeagueConfig,

    // ── Wizard navigation ─────────────────────────────────────────────────────

    /// The currently visible wizard step.
    /// Stored as `Option` only so that `Default` can produce a valid struct;
    /// in practice it is always `Some`.  Use `current_step()` to unwrap safely.
    pub step: Option<Step>,

    // ── Step 1 — Teams ────────────────────────────────────────────────────────

    /// Live contents of the "new team name" text field.
    /// Cleared after each successful `AddTeam`.
    pub new_team_name: String,

    // ── Step 2 — Weeks ────────────────────────────────────────────────────────

    /// Raw text from the weeks input field (e.g. `"1, 2, 3, 4, 5"`).
    /// Parsed into `config.weeks` only when the user clicks Apply.
    pub weeks_input: String,

    /// Validation error message shown beneath the weeks field.
    /// `None` when the last parse succeeded or no parse has been attempted.
    pub weeks_error: Option<String>,

    // ── Step 3 — Base Schedule ────────────────────────────────────────────────

    /// One raw matchup string per week number (e.g. `"A vs B, C vs D"`).
    /// These mirror `config.base_schedule` but in editable string form so
    /// the user can type freely before clicking Apply.
    pub base_schedule_inputs: HashMap<u32, String>,

    /// Validation error for the base-schedule parse attempt.
    pub base_schedule_error: Option<String>,

    // ── Step 4 — Bye Preferences ──────────────────────────────────────────────

    /// Editable preference strings, keyed by team name.
    /// Each value is `[first_pref_str, second_pref_str]`.
    /// Both strings are validated together; only valid pairs are written to
    /// `config.bye_preferences`.
    pub pref_inputs: HashMap<String, [String; 2]>,

    // ── Results ───────────────────────────────────────────────────────────────

    /// Up to five ranked `Solution` values returned by the scheduler.
    /// Empty if the scheduler has not been run or returned an error.
    pub results: Vec<Solution>,

    /// The rank number (1-based) of the solution currently displayed.
    pub selected_rank: usize,

    /// Error string from the last scheduler run, if it failed.
    pub run_error: Option<String>,

    // ── Progress modal ────────────────────────────────────────────────────────

    /// `true` while the background scheduler thread is running.
    /// When true, `view()` replaces the normal UI with a full-screen modal
    /// containing the progress bar.
    pub is_running: bool,

    /// Progress value in [0.0, 1.0] updated by `Message::SchedulerProgress`.
    /// Drives the `progress_bar` widget inside the modal.
    pub scheduler_progress: f32,
}

impl SwimScheduler {
    /// Construct the initial application state and return it alongside an
    /// empty `Task` (no async work needed at startup).
    ///
    /// The default `LeagueConfig` is pre-loaded with a 5-team schedule so the
    /// user can immediately click through the wizard and run the scheduler
    /// without configuring anything.  The `weeks_input` and
    /// `base_schedule_inputs` strings are synchronised with that default so
    /// the text fields reflect the initial config.
    pub fn new() -> (Self, Task<Message>) {
        let config = LeagueConfig::default();

        // Render the default weeks as a comma-separated string for the input field.
        let weeks_input = config.weeks
            .iter()
            .map(|w| w.to_string())
            .collect::<Vec<_>>()
            .join(", ");

        // Convert the structured base schedule into per-week matchup strings.
        let base_schedule_inputs = schedule_to_inputs(&config.base_schedule);

        (
            Self {
                config,
                step: Some(Step::Teams),
                weeks_input,
                base_schedule_inputs,
                selected_rank: 1,
                ..Default::default()
            },
            Task::none(),
        )
    }

    /// Return a reference to the current wizard step.
    ///
    /// Falls back to `Step::Teams` if `self.step` is somehow `None`
    /// (should not happen in normal usage).
    pub fn current_step(&self) -> &Step {
        self.step.as_ref().unwrap_or(&Step::Teams)
    }
}
