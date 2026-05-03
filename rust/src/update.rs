//! # update.rs
//!
//! The iced `update` function and all supporting mutation helpers.
//!
//! ## Architecture
//!
//! iced is a purely functional UI framework: `view()` produces a widget tree,
//! user interactions and async results are converted into `Message` values, and
//! `update()` is the single place where state actually changes.  Every arm of
//! the `match` in `SwimScheduler::update()` corresponds to one `Message` variant
//! defined in `message.rs`.
//!
//! ## Async tasks
//!
//! Several messages trigger async work (file dialogs, the scheduler).  iced
//! handles async via `Task<Message>`: the caller returns a `Task` from `update`,
//! iced drives it on its internal executor, and the result is delivered back as
//! another `Message`.
//!
//! ## Scheduler progress streaming
//!
//! The scheduler is CPU-bound and potentially slow, so it runs on a dedicated
//! OS thread (via `std::thread::spawn`).  Progress values are pushed through an
//! `iced::futures::channel::mpsc` async channel so the UI can update the
//! progress bar in real time.  The final `Result` is passed through an
//! `Arc<Mutex<Option<...>>>` shared slot — the stream reads it when it sees the
//! sentinel value `2.0` on the channel.
//!
//! We use `iced::futures::channel::mpsc` (not `std::sync::mpsc`) because the
//! async stream's `rx.next().await` call integrates natively with iced's
//! executor (which is smol/async-std based, not tokio).  No extra runtime
//! dependency is needed.

use iced::Task;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::state::SwimScheduler;
use crate::message::Message;
use crate::state::Step;
use crate::config::{schedule_to_inputs, LeagueConfig, default_5team_schedule, default_6team_schedule};
use crate::scheduler::{run_scheduler_with_progress, Solution};

// iced re-exports the `futures` crate.  We use its mpsc channel so the async
// stream can `.await` new values without needing tokio.
use iced::futures::channel::mpsc as async_mpsc;
use iced::futures::SinkExt;
use iced::futures::StreamExt;

// ── Main update implementation ────────────────────────────────────────────────

impl SwimScheduler {
    /// Process one `Message` and return any async `Task` that should be run.
    ///
    /// The method signature matches what `iced::application` expects.
    /// Returning `Task::none()` means "nothing async to do"; returning a
    /// concrete `Task` tells iced to drive that future/stream and deliver the
    /// result as another `Message`.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {

            // ── Wizard navigation ─────────────────────────────────────────────

            /// Jump directly to any step (used by breadcrumb buttons).
            Message::GoTo(s) => self.step = Some(s),

            /// Advance one step forward in the wizard sequence.
            /// No-op if already on the last step.
            Message::Next => {
                let steps = Step::all();
                if let Some(pos) = steps.iter().position(|s| s == self.current_step()) {
                    if pos + 1 < steps.len() {
                        self.step = Some(steps[pos + 1].clone());
                    }
                }
            }

            /// Move one step backward in the wizard sequence.
            /// No-op if already on the first step.
            Message::Back => {
                let steps = Step::all();
                if let Some(pos) = steps.iter().position(|s| s == self.current_step()) {
                    if pos > 0 {
                        self.step = Some(steps[pos - 1].clone());
                    }
                }
            }

            // ── Team management ───────────────────────────────────────────────

            /// Keep `new_team_name` in sync with the text field on every keystroke.
            Message::NewTeamNameChanged(s) => self.new_team_name = s,

            /// Validate and commit the current `new_team_name` to `config.teams`.
            /// Silently ignores blank names or names that already exist.
            ///
            /// After adding, re-syncs labels and — if the team count just
            /// reached a supported default size (5 or 6) — automatically loads
            /// the corresponding built-in round-robin base schedule.  This
            /// saves the user from having to visit Step 3 and click a default
            /// button manually in the common case.
            Message::AddTeam => {
                let name = self.new_team_name.trim().to_string();
                // 6 is the maximum supported team count.  The guard is checked
                // before anything else so the name field is left intact — the
                // user can correct the situation by removing a team first.
                if !name.is_empty()
                    && !self.config.teams.contains(&name)
                    && self.config.teams.len() < 6
                {
                    self.config.teams.push(name);
                    self.new_team_name.clear();
                    self.sync_labels(); // keep labels.len() == teams.len()
                    // Auto-load the matching default schedule whenever the team
                    // count lands on a supported size.  The user can still
                    // override it manually on the Base Schedule step.
                    self.apply_default_schedule_if_supported();
                }
            }

            /// Remove the team at `idx` and re-sync labels.
            /// Bounds-checked — ignores out-of-range indices.
            ///
            /// Also re-applies the matching default schedule if the new team
            /// count is 5 or 6, so the base schedule stays consistent as the
            /// user adjusts the team list.
            Message::RemoveTeam(idx) => {
                if idx < self.config.teams.len() {
                    self.config.teams.remove(idx);
                    self.sync_labels();
                    self.apply_default_schedule_if_supported();
                }
            }

            // ── Config persistence ────────────────────────────────────────────

            /// Serialise `config` to pretty-printed JSON and write it to a
            /// user-chosen file.
            ///
            /// Opens a native save-file dialog asynchronously (via `rfd`).
            /// If the user cancels, the `ConfigSaved` result carries `Err("Cancelled")`.
            Message::SaveConfig => {
                let config = self.config.clone();
                return Task::perform(
                    async move {
                        let path = rfd::AsyncFileDialog::new()
                            .set_file_name("league.json")
                            .add_filter("JSON", &["json"])
                            .save_file()
                            .await;
                        match path {
                            None => Err("Cancelled".into()),
                            Some(p) => {
                                let json = serde_json::to_string_pretty(&config)
                                    .map_err(|e| e.to_string())?;
                                std::fs::write(p.path(), json).map_err(|e| e.to_string())
                            }
                        }
                    },
                    Message::ConfigSaved,
                );
            }

            /// Pick a JSON file and deserialise it into a `LeagueConfig`.
            ///
            /// On success the result arrives as `ConfigLoaded(Ok(cfg))` and the
            /// entire config is replaced atomically in that arm below.
            Message::LoadConfig => {
                return Task::perform(
                    async {
                        let path = rfd::AsyncFileDialog::new()
                            .add_filter("JSON", &["json"])
                            .pick_file()
                            .await;
                        match path {
                            None => Err("Cancelled".into()),
                            Some(p) => {
                                let bytes = std::fs::read(p.path())
                                    .map_err(|e| e.to_string())?;
                                serde_json::from_slice::<LeagueConfig>(&bytes)
                                    .map_err(|e| e.to_string())
                            }
                        }
                    },
                    Message::ConfigLoaded,
                );
            }

            /// Save result — currently ignored (could show a toast in future).
            Message::ConfigSaved(_) => {}

            /// Replace all app state with the freshly loaded config.
            /// Also re-derives the text-field strings so the UI reflects the
            /// loaded values immediately.
            Message::ConfigLoaded(Ok(cfg)) => {
                self.config = cfg;
                // Rebuild the weeks text field from the loaded week list.
                self.weeks_input = self.config.weeks
                    .iter()
                    .map(|w| w.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                // Rebuild per-week matchup strings from the loaded base schedule.
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                // Rebuild preference text-field strings from loaded preferences.
                self.pref_inputs = self.config.bye_preferences
                    .iter()
                    .map(|(k, &[a, b])| (k.clone(), [a.to_string(), b.to_string()]))
                    .collect();
            }

            /// Load failed or was cancelled — silently ignore.
            /// In a production app this would show an error notification.
            Message::ConfigLoaded(Err(_)) => {}

            // ── Week configuration ────────────────────────────────────────────

            /// Keep `weeks_input` in sync with the text field on every keystroke.
            Message::WeeksInputChanged(s) => self.weeks_input = s,

            /// Parse `weeks_input` as a comma-separated list of u32 week numbers.
            /// On success: clear any error, write to `config.weeks`, and
            /// re-sync the base-schedule input strings (in case the week set
            /// changed).  On failure: set a descriptive error message.
            Message::ApplyWeeks => {
                let parsed: Result<Vec<u32>, _> = self.weeks_input
                    .split(',')
                    .map(|s| s.trim().parse::<u32>())
                    .collect();
                match parsed {
                    Ok(w) if !w.is_empty() => {
                        self.weeks_error = None;
                        self.config.weeks = w;
                        self.sync_base_schedule_inputs();
                    }
                    _ => {
                        self.weeks_error = Some(
                            "Enter comma-separated week numbers, e.g. 1, 2, 3, 4, 5".into(),
                        );
                    }
                }
            }

            // ── Base schedule ─────────────────────────────────────────────────

            /// Update the raw matchup string for a single week without parsing.
            /// Parsing is deferred until the user clicks Apply.
            Message::BaseScheduleInputChanged(w, s) => {
                self.base_schedule_inputs.insert(w, s);
            }

            /// Parse all `base_schedule_inputs` and write to `config.base_schedule`.
            /// Sets `base_schedule_error` on any parse failure so the view can
            /// display the offending week.
            Message::ApplyBaseSchedule => {
                match self.parse_base_schedule() {
                    Ok(bs) => {
                        self.config.base_schedule = bs;
                        self.base_schedule_error = None;
                    }
                    Err(e) => self.base_schedule_error = Some(e),
                }
            }

            /// Overwrite labels and base schedule with the built-in 5-team
            /// round-robin (labels A–E, 5 weeks, each team plays every other
            /// team exactly once with one bye per team).
            Message::UseDefault5Team => {
                self.config.labels = ["A", "B", "C", "D", "E"]
                    .iter().map(|s| s.to_string()).collect();
                self.config.base_schedule = default_5team_schedule();
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.base_schedule_error = None;
            }

            /// Same as above for the 6-team round-robin (labels A–F, 5 weeks,
            /// every team plays every week — no byes in a 6-team schedule).
            Message::UseDefault6Team => {
                self.config.labels = ["A", "B", "C", "D", "E", "F"]
                    .iter().map(|s| s.to_string()).collect();
                self.config.base_schedule = default_6team_schedule();
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.base_schedule_error = None;
            }

            // ── Bye preferences ───────────────────────────────────────────────

            /// Update one preference slot for a team.
            /// `idx` is 0 for the 1st-choice field, 1 for the 2nd-choice field.
            ///
            /// The raw string is stored immediately so the text field stays
            /// responsive.  Only when *both* slots parse as valid `u32` values
            /// are they written to `config.bye_preferences`.
            Message::PrefChanged(team, idx, val) => {
                let entry = self.pref_inputs
                    .entry(team.clone())
                    .or_insert(["".into(), "".into()]);
                entry[idx] = val;
                let a = entry[0].trim().parse::<u32>();
                let b = entry[1].trim().parse::<u32>();
                if let (Ok(f), Ok(s)) = (a, b) {
                    self.config.bye_preferences.insert(team, [f, s]);
                }
            }

            // ── Bye restrictions ──────────────────────────────────────────────

            /// Toggle a bye restriction for (team, week).
            ///
            /// If `week` is already in the team's restriction list it is removed
            /// (unchecked); otherwise it is appended (checked).
            Message::ToggleRestriction(team, week) => {
                let list = self.config.bye_restrictions.entry(team).or_default();
                if let Some(pos) = list.iter().position(|&w| w == week) {
                    list.remove(pos);
                } else {
                    list.push(week);
                }
            }

            // ── Score exclusions ──────────────────────────────────────────────

            /// Toggle whether a team is excluded from the fairness-score
            /// calculation.  Excluded teams still get byes — they just don't
            /// affect which schedule ranks highest.
            Message::ToggleExclusion(team) => {
                if let Some(pos) = self.config.score_excluded.iter().position(|t| t == &team) {
                    self.config.score_excluded.remove(pos);
                } else {
                    self.config.score_excluded.push(team);
                }
            }

            // ── Scheduler ─────────────────────────────────────────────────────

            /// Launch the scheduler on a background OS thread and open the
            /// progress modal.
            ///
            /// ## Channel design
            ///
            /// We need to bridge a blocking CPU-bound thread with iced's async
            /// event loop.  The approach:
            ///
            /// 1. Create an `iced::futures::channel::mpsc` bounded async channel
            ///    with capacity 64.  The scheduler thread sends `f32` progress
            ///    values (0.0–1.0) via non-blocking `try_send`.  When done it
            ///    sends a sentinel value `2.0`.
            ///
            /// 2. The final `Result` (which is not `f32`) is stored in a shared
            ///    `Arc<Mutex<Option<Result<...>>>>` slot.  The scheduler thread
            ///    fills it just before sending the sentinel; the async stream
            ///    reads it when it sees the sentinel.
            ///
            /// 3. `Task::run` wraps the receiver in an `unfold` stream, mapping
            ///    each channel value to a `Message`:
            ///    - `v < 1.5` → `SchedulerProgress(v)`
            ///    - `v >= 1.5` (sentinel) → `SchedulerComplete(result_slot.take())`
            ///
            /// ## Why not tokio?
            ///
            /// iced 0.13 uses smol/async-std internally, not tokio.  Using
            /// `tokio::task::spawn_blocking` would require adding tokio as a
            /// dependency and risking two runtimes.  The `async_mpsc` channel
            /// from `iced::futures` integrates natively with the existing
            /// executor at zero extra cost.
            Message::RunScheduler => {
                self.is_running = true;
                self.scheduler_progress = 0.0;

                let config = self.config.clone();

                // Shared result slot: the scheduler thread writes here just
                // before sending the sentinel; the stream reads it once.
                let result_slot: Arc<Mutex<Option<Result<Vec<Solution>, String>>>> =
                    Arc::new(Mutex::new(None));
                let result_slot_thread = Arc::clone(&result_slot);

                // Bounded async mpsc channel.
                // Capacity 64 means we buffer up to 64 progress ticks before
                // try_send starts dropping ticks — acceptable since the bar only
                // needs to move visually, not record every single tick.
                let (mut tx, rx) = async_mpsc::channel::<f32>(64);
                let mut tx_done = tx.clone();

                // Spawn a plain OS thread so the blocking rayon work never
                // touches the async executor's thread pool.
                std::thread::spawn(move || {
                    let result = run_scheduler_with_progress(&config, move |p| {
                        // try_send is non-blocking: if the buffer is full we
                        // just skip this tick rather than blocking the scheduler.
                        let _ = tx.try_send(p);
                    });
                    // Store the result before sending the sentinel so the stream
                    // always finds it ready when it reads the slot.
                    *result_slot_thread.lock().unwrap() = Some(result);
                    let _ = tx_done.try_send(2.0); // sentinel: work is done
                });

                // Build a stream that drains the channel and maps values to
                // Messages.  `unfold` carries (rx, result_slot) as state between
                // iterations so both remain accessible throughout the stream's
                // lifetime.
                return Task::run(
                    iced::futures::stream::unfold(
                        (rx, result_slot),
                        |(mut rx, slot)| async move {
                            match rx.next().await {
                                Some(v) if v < 1.5 => {
                                    // Normal progress tick — forward to the update loop.
                                    Some((Message::SchedulerProgress(v), (rx, slot)))
                                }
                                Some(_) => {
                                    // Sentinel received.  Extract the result that the
                                    // scheduler thread stored in the shared slot.
                                    let result = slot
                                        .lock()
                                        .unwrap()
                                        .take()
                                        .unwrap_or_else(|| Err("No result".into()));
                                    Some((Message::SchedulerComplete(result), (rx, slot)))
                                }
                                // Channel closed with no more values — end the stream.
                                None => None,
                            }
                        },
                    ),
                    std::convert::identity,
                );
            }

            /// Intermediate progress update from the background scheduler thread.
            /// Simply stores the new value; `view()` reads it to update the bar.
            Message::SchedulerProgress(p) => {
                self.scheduler_progress = p;
            }

            /// The scheduler thread finished.
            /// Closes the progress modal, navigates to Results, and either
            /// stores the solutions or records the error for display.
            Message::SchedulerComplete(result) => {
                self.is_running = false;
                self.scheduler_progress = 1.0;
                self.step = Some(Step::Results);
                match result {
                    Ok(sols) => {
                        self.results = sols;
                        self.selected_rank = 1;
                        self.run_error = None;
                    }
                    Err(e) => {
                        self.results.clear();
                        self.run_error = Some(e);
                    }
                }
            }

            // ── Results view ──────────────────────────────────────────────────

            /// Switch the results panel to display the solution with the given
            /// 1-based rank number.
            Message::SelectRank(r) => self.selected_rank = r,

            // ── Export ────────────────────────────────────────────────────────

            /// Write all ranked solutions to a user-chosen plain-text file.
            ///
            /// The output format is human-readable: one section per solution
            /// with bye-week assignments and a week-by-week match listing.
            Message::ExportResults => {
                let results = self.results.clone();
                let weeks   = self.config.weeks.clone();
                let teams   = self.config.teams.clone();
                return Task::perform(
                    async move {
                        let path = rfd::AsyncFileDialog::new()
                            .set_file_name("schedules.txt")
                            .add_filter("Text", &["txt"])
                            .save_file()
                            .await;
                        match path {
                            None => Err("Cancelled".into()),
                            Some(p) => {
                                let mut out = String::new();
                                for sol in &results {
                                    out.push_str(&format!(
                                        "\n==============================\nRank: {}\nScore: {}\n\nBye Weeks\n",
                                        sol.rank, sol.score
                                    ));
                                    for t in &teams {
                                        out.push_str(&format!(
                                            "  {}: week {}  (score {})\n",
                                            t,
                                            sol.bye_assignment.get(t).copied().unwrap_or(0),
                                            sol.bye_detail.get(t).copied().unwrap_or(0)
                                        ));
                                    }
                                    out.push_str("\nMatches\n");
                                    let mut sw = weeks.clone();
                                    sw.sort();
                                    for w in &sw {
                                        out.push_str(&format!("\n  Week {}\n", w));
                                        if let Some(games) = sol.schedule.get(w) {
                                            for (h, a) in games {
                                                out.push_str(&format!("    {} hosts {}\n", h, a));
                                            }
                                        }
                                    }
                                }
                                std::fs::write(p.path(), out).map_err(|e| e.to_string())
                            }
                        }
                    },
                    Message::ExportDone,
                );
            }

            /// Export result — currently ignored (could show a success/error
            /// notification in a future iteration).
            Message::ExportDone(_) => {}
        }

        // Default: no async work to do.
        Task::none()
    }
}

// ── Sync helper methods ───────────────────────────────────────────────────────

impl SwimScheduler {
    /// Rebuild `config.labels` to match the current team count.
    ///
    /// Labels are single uppercase letters A–H assigned in order.  If there
    /// are more than 8 teams the excess teams get the last label — in practice
    /// the scheduler only supports 5 or 6 teams so this is a safety cap.
    ///
    /// Called after every `AddTeam` / `RemoveTeam` so `labels.len()` always
    /// equals `teams.len()`.
    pub fn sync_labels(&mut self) {
        let chars = ['A', 'B', 'C', 'D', 'E', 'F', 'G', 'H'];
        self.config.labels = chars[..self.config.teams.len().min(chars.len())]
            .iter()
            .map(|c| c.to_string())
            .collect();
    }

    /// If the current team count matches a supported default size (5 or 6),
    /// load the corresponding built-in round-robin base schedule and sync the
    /// text-field representations.
    ///
    /// Called automatically after every `AddTeam` and `RemoveTeam` so the base
    /// schedule always reflects the team count without requiring the user to
    /// visit Step 3 and click a default button manually.
    ///
    /// This is a no-op when the team count is anything other than 5 or 6,
    /// preserving any custom schedule the user may have entered for other sizes.
    pub fn apply_default_schedule_if_supported(&mut self) {
        match self.config.teams.len() {
            5 => {
                // Switch to the 5-team round-robin: labels A–E, one bye per
                // team per week, every pair plays exactly once.
                self.config.labels = ["A", "B", "C", "D", "E"]
                    .iter().map(|s| s.to_string()).collect();
                self.config.base_schedule = default_5team_schedule();
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.base_schedule_error = None;
            }
            6 => {
                // Switch to the 6-team round-robin: labels A–F, no byes,
                // every team plays every week.
                self.config.labels = ["A", "B", "C", "D", "E", "F"]
                    .iter().map(|s| s.to_string()).collect();
                self.config.base_schedule = default_6team_schedule();
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.base_schedule_error = None;
            }
            // Any other count (< 5, or > 6): leave the schedule untouched.
            _ => {}
        }
    }
    ///
    /// Called after `ApplyWeeks` so that any week added or removed from the
    /// week list gets a corresponding (possibly empty) text field in the UI.
    pub fn sync_base_schedule_inputs(&mut self) {
        self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
    }

    /// Parse `base_schedule_inputs` into structured matchup data.
    ///
    /// Each week's string is expected to contain comma-separated `"X vs Y"`
    /// pairs, where X and Y are label strings (e.g. `"A vs B, C vs D"`).
    ///
    /// Returns `Err` with a human-readable message on the first parse failure,
    /// including which week and which matchup string caused the error.
    pub fn parse_base_schedule(&self) -> Result<HashMap<u32, Vec<[String; 2]>>, String> {
        let mut result = HashMap::new();
        for (&w, raw) in &self.base_schedule_inputs {
            let matchups: Result<Vec<[String; 2]>, String> = raw
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|m| {
                    // Expect exactly one "vs" separator with non-empty sides.
                    let parts: Vec<&str> = m.split("vs").map(|s| s.trim()).collect();
                    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
                        Err(format!("Invalid matchup '{}' in week {}", m.trim(), w))
                    } else {
                        Ok([parts[0].to_string(), parts[1].to_string()])
                    }
                })
                .collect();
            result.insert(w, matchups?);
        }
        Ok(result)
    }
}
