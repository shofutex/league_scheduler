//! # message.rs
//!
//! Defines every event that can flow through the iced update loop.
//!
//! iced is an Elm-architecture framework: user interactions and async
//! results are converted into `Message` variants, which are handed to
//! `SwimScheduler::update()` one at a time on the main thread.  Nothing
//! mutable happens anywhere else — all state changes go through here.

use crate::config::LeagueConfig;
use crate::state::Step;
use crate::scheduler::Solution;

#[derive(Debug, Clone)]
pub enum Message {
    // ── Wizard navigation ────────────────────────────────────────────────────

    /// Jump directly to a named wizard step (used by the breadcrumb buttons).
    GoTo(Step),

    /// Advance to the next step in the linear wizard sequence.
    Next,

    /// Return to the previous step in the linear wizard sequence.
    Back,

    // ── Team management (Step 1) ─────────────────────────────────────────────

    /// Fired on every keystroke in the "new team name" text input.
    NewTeamNameChanged(String),

    /// Commit the current `new_team_name` to `config.teams` (also bound to
    /// the Enter key via `on_submit`).
    AddTeam,

    /// Remove the team at the given index from `config.teams`.
    RemoveTeam(usize),

    // ── Config persistence ───────────────────────────────────────────────────

    /// Open a native save-file dialog and serialise `config` to JSON.
    SaveConfig,

    /// Open a native open-file dialog and deserialise a `LeagueConfig` from JSON.
    LoadConfig,

    /// Result delivered back to the update loop after the async save completes.
    /// `Err` carries a human-readable reason (including "Cancelled").
    ConfigSaved(Result<(), String>),

    /// Result delivered back after the async load completes.
    /// On success the full `LeagueConfig` is included so `update` can
    /// replace the current config atomically.
    ConfigLoaded(Result<LeagueConfig, String>),

    // ── Week configuration (Step 2) ──────────────────────────────────────────

    /// Live text-input updates for the comma-separated week list.
    WeeksInputChanged(String),

    /// Parse and validate `weeks_input`; update `config.weeks` if valid.
    ApplyWeeks,

    // ── Base schedule (Step 3) ───────────────────────────────────────────────

    /// Live text-input update for the matchup string of a single week.
    /// The `u32` is the week number whose field changed.
    BaseScheduleInputChanged(u32, String),

    /// Parse all `base_schedule_inputs` and update `config.base_schedule`.
    ApplyBaseSchedule,

    /// Replace the current base schedule and labels with the built-in
    /// round-robin for exactly 5 teams (A–E).
    UseDefault5Team,

    /// Replace the current base schedule and labels with the built-in
    /// round-robin for exactly 6 teams (A–F).
    UseDefault6Team,

    // ── Bye preferences (Step 4) ─────────────────────────────────────────────

    /// One of a team's preference text inputs changed.
    /// Fields: team name, slot index (0 = 1st pref, 1 = 2nd pref), new value.
    PrefChanged(String, usize, String),

    // ── Bye restrictions (Step 5) ────────────────────────────────────────────

    /// Toggle whether `team` is restricted from taking a bye in `week`.
    /// If the week is already in the restriction list it is removed; otherwise
    /// it is added.
    ToggleRestriction(String, u32),

    // ── Score exclusions (Step 6) ────────────────────────────────────────────

    /// Toggle whether `team` is excluded from the fairness-score calculation.
    /// Excluded teams still receive a bye — they just don't influence which
    /// schedule ranks highest.
    ToggleExclusion(String),

    // ── Scheduler execution ──────────────────────────────────────────────────

    /// Kick off the scheduling algorithm on a background thread and open the
    /// progress modal.  The result arrives later via `SchedulerComplete`.
    RunScheduler,

    /// Intermediate progress update emitted by the background thread.
    /// Value is in [0.0, 1.0]; drives the progress bar in the modal.
    SchedulerProgress(f32),

    /// Final result from the background scheduler thread.
    /// On success, contains the ranked list of solutions (up to 5).
    /// On failure, contains a human-readable error string.
    SchedulerComplete(Result<Vec<Solution>, String>),

    // ── Results view ─────────────────────────────────────────────────────────

    /// Switch the results view to show the schedule with the given rank number.
    SelectRank(usize),

    // ── Export ───────────────────────────────────────────────────────────────

    /// Open a native save-file dialog and write all ranked schedules to a
    /// plain-text `.txt` file.
    ExportResults,

    /// Delivered after the async export completes (success or failure).
    ExportDone(Result<(), String>),
}
