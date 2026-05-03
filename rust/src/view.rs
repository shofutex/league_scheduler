//! # view.rs
//!
//! The iced `view` function — pure, stateless rendering of `SwimScheduler`.
//!
//! ## Structure
//!
//! `SwimScheduler::view()` is the top-level entry point called by iced on every
//! render cycle.  It delegates to one of seven per-step helpers, wraps the
//! result in a shared chrome (header + breadcrumb + nav bar), and — if the
//! scheduler is currently running — replaces the entire UI with a full-screen
//! progress modal.
//!
//! ## Progress modal
//!
//! When `self.is_running` is `true`, `view_progress_modal()` is returned
//! instead of the normal wizard layout.  The modal shows a progress bar driven
//! by `self.scheduler_progress` (updated by `Message::SchedulerProgress`
//! streaming in from the background thread) and a phase label that changes as
//! the bar crosses thresholds.
//!
//! ## Widget conventions
//!
//! - All widgets produce `Element<'_, Message>` and are composed bottom-up.
//! - `scrollable(...)` wraps every step's content so it works on small windows.
//! - Team names are coloured using `team_color()` from `config.rs` to make
//!   them visually distinct throughout the wizard.

use iced::widget::{
    button, checkbox, column, container, horizontal_rule, progress_bar,
    row, scrollable, text, text_input, Space,
};
use iced::{color, Alignment, Color, Element, Length};

use crate::config::team_color;
use crate::message::Message;
use crate::state::{Step, SwimScheduler};

// ── Top-level view ────────────────────────────────────────────────────────────

impl SwimScheduler {
    /// Build the complete widget tree for the current application state.
    ///
    /// The modal check happens first: if the scheduler is running we skip
    /// the normal chrome entirely and return the progress overlay.  This
    /// prevents the user from interacting with the wizard while work is in
    /// progress.
    pub fn view(&self) -> Element<'_, Message> {
        // Assemble the normal wizard UI regardless — the modal path discards
        // it, but computing it here keeps the branching logic simple.
        let main_ui: Element<Message> = column![
            self.view_header(),
            self.view_breadcrumb(),
            horizontal_rule(1),
            // Delegate to the per-step content helper.
            match self.current_step() {
                Step::Teams           => self.view_teams(),
                Step::Weeks           => self.view_weeks(),
                Step::BaseSchedule    => self.view_base_schedule(),
                Step::ByePreferences  => self.view_bye_preferences(),
                Step::ByeRestrictions => self.view_bye_restrictions(),
                Step::ScoreExclusions => self.view_score_exclusions(),
                Step::Results         => self.view_results(),
            },
            horizontal_rule(1),
            self.view_nav(),
        ]
        .into();

        if self.is_running {
            // Replace the whole UI with the progress modal.
            self.view_progress_modal(main_ui)
        } else {
            main_ui
        }
    }

    // ── Progress modal ────────────────────────────────────────────────────────

    /// Full-screen progress overlay shown while the scheduler runs.
    ///
    /// iced 0.13 does not have a built-in `Stack` or `Overlay` widget, so we
    /// simulate a modal by replacing the entire view with a dark-background
    /// container that centres the card.  The `base` parameter (the normal UI)
    /// is discarded here; it exists in the signature so the caller can build
    /// it naturally and pass it in without knowing whether it will be shown.
    ///
    /// The phase label switches at progress thresholds to give the user a
    /// rough sense of which part of the algorithm is running:
    /// - 0 – 9 %  → "Preparing…"         (validation + structural pre-compute)
    /// - 10 – 89% → "Searching schedules…" (parallel permutation enumeration)
    /// - 90 – 100% → "Finalising results…" (dedup + ranking)
    fn view_progress_modal<'a>(&self, _base: Element<'a, Message>) -> Element<'a, Message> {
        let pct = (self.scheduler_progress * 100.0).round() as u32;

        let phase_label = if pct < 10 {
            "Preparing…"
        } else if pct < 90 {
            "Searching schedules…"
        } else {
            "Finalising results…"
        };

        // ── Inner card ────────────────────────────────────────────────────────
        // A rounded, shadowed box containing the title, phase label, bar, and %.
        let card: Element<Message> = container(
            column![
                text("Running Scheduler").size(18),
                Space::with_height(8),
                // Subdued colour for the phase label so it doesn't compete
                // with the title.
                text(phase_label).size(13).color(Color {
                    r: 0.6, g: 0.6, b: 0.6, a: 1.0,
                }),
                Space::with_height(16),
                // iced's built-in progress_bar widget.  Range 0.0..=1.0 maps
                // directly to `scheduler_progress`.
                progress_bar(0.0..=1.0, self.scheduler_progress)
                    .width(Length::Fill)
                    .height(12),
                Space::with_height(10),
                text(format!("{}%", pct)).size(13),
            ]
            .spacing(0)
            .align_x(Alignment::Center)
            .width(340),
        )
        .padding(32)
        // Custom dark card style matching the Tokyo Night theme.
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(Color {
                r: 0.13, g: 0.14, b: 0.20, a: 1.0,
            })),
            border: iced::Border {
                color: Color { r: 0.25, g: 0.27, b: 0.38, a: 1.0 },
                width: 1.0,
                radius: 10.0.into(),
            },
            shadow: iced::Shadow {
                color: Color { r: 0.0, g: 0.0, b: 0.0, a: 0.5 },
                offset: iced::Vector { x: 0.0, y: 4.0 },
                blur_radius: 20.0,
            },
            ..Default::default()
        })
        .into();

        // ── Full-screen backdrop ──────────────────────────────────────────────
        // Fill the window with a dark solid colour and centre the card using
        // Flexbox-style spacers (Fill + Fill around the card column/row).
        container(
            column![
                Space::with_height(Length::Fill),
                row![
                    Space::with_width(Length::Fill),
                    card,
                    Space::with_width(Length::Fill),
                ],
                Space::with_height(Length::Fill),
            ],
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|_theme| container::Style {
            background: Some(iced::Background::Color(Color {
                r: 0.07, g: 0.08, b: 0.12, a: 1.0,
            })),
            ..Default::default()
        })
        .into()
    }

    // ── Shared chrome ─────────────────────────────────────────────────────────

    /// Top header: app title + current step title.
    fn view_header(&self) -> Element<'_, Message> {
        container(
            column![
                text("Swim League Scheduler").size(28),
                text(self.current_step().title()).size(14),
            ]
            .spacing(4),
        )
        .padding(20)
        .into()
    }

    /// Breadcrumb navigation bar: one button per step.
    ///
    /// The active step uses the default (primary) button style; all others
    /// use the secondary style so the current position is obvious at a glance.
    /// A `›` separator is inserted between steps.
    fn view_breadcrumb(&self) -> Element<'_, Message> {
        let current = self.current_step();
        let items: Vec<Element<Message>> = Step::all()
            .iter()
            .enumerate()
            .map(|(i, s)| {
                // Short labels keep the bar from wrapping on 900 px wide windows.
                let label = match s {
                    Step::Teams           => "Teams",
                    Step::Weeks           => "Weeks",
                    Step::BaseSchedule    => "Schedule",
                    Step::ByePreferences  => "Prefs",
                    Step::ByeRestrictions => "Restrictions",
                    Step::ScoreExclusions => "Exclusions",
                    Step::Results         => "Results",
                };
                let btn: Element<Message> = if s == current {
                    button(text(label).size(13))
                        .on_press(Message::GoTo(s.clone()))
                        .into()
                } else {
                    button(text(label).size(13))
                        .style(button::secondary)
                        .on_press(Message::GoTo(s.clone()))
                        .into()
                };
                // Append a › separator after every step except the last.
                if i < Step::all().len() - 1 {
                    row![btn, text(" › ").size(13)]
                        .align_y(Alignment::Center)
                        .into()
                } else {
                    btn
                }
            })
            .collect();

        container(row(items).spacing(4).align_y(Alignment::Center))
            .padding([6, 20])
            .into()
    }

    /// Bottom navigation bar: Back / Load / Save / Next (or Run / Export).
    ///
    /// The Back button is disabled (no `on_press`) on the first step.
    /// The right-hand action button changes based on the current step:
    /// - Step 6 (ScoreExclusions) → "Run Scheduler →" (triggers the engine).
    /// - Results → "Export Results".
    /// - All others → "Next →".
    fn view_nav(&self) -> Element<'_, Message> {
        let steps = Step::all();
        let pos = steps
            .iter()
            .position(|s| s == self.current_step())
            .unwrap_or(0);

        // Back is visually present but not interactive on the first step.
        let back: Element<Message> = if pos > 0 {
            button(text("← Back"))
                .style(button::secondary)
                .on_press(Message::Back)
                .into()
        } else {
            button(text("← Back")).style(button::secondary).into()
        };

        // Right-side action varies by step.
        //
        // On the Teams step, Next is disabled until at least 5 teams have been
        // added (5 is the minimum the scheduler supports).  In iced, a button
        // with no `on_press` is rendered in a visually muted style and receives
        // no pointer events — no separate "disabled" flag is needed.
        let right: Element<Message> = match self.current_step() {
            Step::Results => button(text("Export Results"))
                .on_press(Message::ExportResults)
                .into(),
            Step::ScoreExclusions => button(text("Run Scheduler →"))
                .on_press(Message::RunScheduler)
                .into(),
            Step::Teams => {
                let btn = button(text("Next →"));
                if self.config.teams.len() >= 5 {
                    btn.on_press(Message::Next).into()
                } else {
                    // No on_press → button is inert and styled as disabled.
                    btn.into()
                }
            }
            _ => button(text("Next →")).on_press(Message::Next).into(),
        };

        container(
            row![
                back,
                Space::with_width(Length::Fill),
                // Load / Save are always available so the user can persist or
                // restore config from any step.
                button(text("⬒ Load"))
                    .style(button::secondary)
                    .on_press(Message::LoadConfig),
                button(text("↓ Save"))
                    .style(button::secondary)
                    .on_press(Message::SaveConfig),
                right,
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([12, 20])
        .into()
    }

    // ── Step views ────────────────────────────────────────────────────────────

    /// Step 1 — Team list with add/remove controls.
    ///
    /// Each team is shown in its assigned colour (from `team_color()`).
    /// The text input submits on Enter (via `on_submit`) as well as the button.
    fn view_teams(&self) -> Element<'_, Message> {
        let mut list = column![text("Teams").size(16)].spacing(8);
        for (i, team) in self.config.teams.iter().enumerate() {
            let c = team_color(&self.config.teams, team);
            list = list.push(
                row![
                    text(format!("{}. {}", i + 1, team))
                        .color(c)
                        .width(Length::Fill),
                    button(text("Remove"))
                        .style(button::danger)
                        .on_press(Message::RemoveTeam(i)),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }

        let at_max = self.config.teams.len() >= 6;

        // When at the 6-team maximum, the text input and Add button are both
        // rendered without interaction handlers so they appear and behave as
        // disabled.  The update-layer guard in AddTeam provides a second line
        // of defence if the user somehow submits anyway.
        let name_input = text_input("New team name…", &self.new_team_name).width(300);
        let name_input = if at_max {
            name_input // no on_input / on_submit — field is inert
        } else {
            name_input
                .on_input(Message::NewTeamNameChanged)
                .on_submit(Message::AddTeam)
        };
        let add_btn: Element<Message> = if at_max {
            button(text("Add Team")).into() // no on_press — button is inert
        } else {
            button(text("Add Team")).on_press(Message::AddTeam).into()
        };

        let add_row = row![name_input, add_btn]
            .spacing(10)
            .align_y(Alignment::Center);

        scrollable(
            container(
                column![
                    list,
                    add_row,
                    // Status hint covering all three states:
                    //   < 5 teams → amber warning explaining why Next is grey.
                    //   5 teams   → neutral confirmation, ready to proceed.
                    //   6 teams   → neutral confirmation, maximum reached.
                    if self.config.teams.len() < 5 {
                        text(format!(
                            "{} team(s) added — add {} more to continue (minimum 5).",
                            self.config.teams.len(),
                            5 - self.config.teams.len()
                        ))
                        .size(12)
                        .color(Color { r: 0.9, g: 0.6, b: 0.3, a: 1.0 }) // amber warning
                    } else if self.config.teams.len() < 6 {
                        text(format!(
                            "{} team(s) added. You can add one more or proceed.",
                            self.config.teams.len()
                        ))
                        .size(12)
                    } else {
                        text("6 team(s) added — maximum reached.")
                            .size(12)
                    },
                ]
                .spacing(20),
            )
            .padding(20)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    /// Step 2 — Comma-separated week number input.
    ///
    /// The error message is shown in red beneath the input when the last
    /// Apply attempt failed to parse.
    fn view_weeks(&self) -> Element<'_, Message> {
        // Show the error string in red, or an invisible spacer if there is none.
        let err: Element<Message> = if let Some(e) = &self.weeks_error {
            text(e).color(color!(0xff6666)).into()
        } else {
            Space::with_height(0).into()
        };

        scrollable(
            container(
                column![
                    text("Enter week numbers separated by commas:").size(16),
                    row![
                        text_input("e.g. 1, 2, 3, 4, 5", &self.weeks_input)
                            .on_input(Message::WeeksInputChanged)
                            .on_submit(Message::ApplyWeeks)
                            .width(300),
                        button(text("Apply")).on_press(Message::ApplyWeeks),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                    err,
                    // Echo the currently committed week list so the user can
                    // see what will be used by the scheduler.
                    text(format!(
                        "Weeks: {}",
                        self.config
                            .weeks
                            .iter()
                            .map(|w| w.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))
                    .size(13),
                ]
                .spacing(16),
            )
            .padding(20)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    /// Step 3 — Per-week matchup string editor.
    ///
    /// Each week in the current week list gets its own text field pre-populated
    /// from `base_schedule_inputs`.  The user types `"A vs B, C vs D"` style
    /// strings; Apply parses them all.
    ///
    /// Two "Load Default" buttons shortcut to the built-in 5- and 6-team
    /// round-robin schedules so users don't need to type anything.
    fn view_base_schedule(&self) -> Element<'_, Message> {
        let mut sorted_weeks = self.config.weeks.clone();
        sorted_weeks.sort();

        let mut weeks_col = column![].spacing(10);
        for w in &sorted_weeks {
            let val = self.base_schedule_inputs.get(w).cloned().unwrap_or_default();
            let week = *w;
            weeks_col = weeks_col.push(
                row![
                    // Fixed-width label so all text fields align.
                    text(format!("Week {}:", w)).width(70),
                    text_input("A vs B, C vs D", &val)
                        // Capture `week` by value so the closure is independent
                        // per iteration (avoids borrow issues with the loop var).
                        .on_input(move |s| Message::BaseScheduleInputChanged(week, s))
                        .width(Length::Fill),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }

        let err: Element<Message> = if let Some(e) = &self.base_schedule_error {
            text(e).color(color!(0xff6666)).into()
        } else {
            Space::with_height(0).into()
        };

        scrollable(
            container(
                column![
                    text("For each week, enter matchups like: A vs B, C vs D\nOne team per week sits out (bye).").size(13),
                    row![
                        button(text("Load 5-Team Default"))
                            .style(button::secondary)
                            .on_press(Message::UseDefault5Team),
                        button(text("Load 6-Team Default"))
                            .style(button::secondary)
                            .on_press(Message::UseDefault6Team),
                    ]
                    .spacing(10),
                    weeks_col,
                    button(text("Apply Schedule")).on_press(Message::ApplyBaseSchedule),
                    err,
                    // Reminder of which labels are active (derived from team count).
                    text(format!("Labels: {}", self.config.labels.join(", "))).size(12),
                ]
                .spacing(16),
            )
            .padding(20)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }

    /// Step 4 — Per-team bye-week preference editor.
    ///
    /// Each team gets two narrow number fields: 1st choice and 2nd choice.
    /// Both fields must parse before the values are stored in `config`.
    /// Team names are coloured for quick identification.
    fn view_bye_preferences(&self) -> Element<'_, Message> {
        let mut rows = column![text(
            "Enter each team's 1st and 2nd preferred bye week. Leave blank for no preference."
        )
        .size(13)]
        .spacing(10);

        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            // Resolve the display value: prefer the live text-field string
            // (which may be partially typed), fall back to the committed config
            // value, then fall back to empty.
            let defaults = self
                .pref_inputs
                .get(team)
                .cloned()
                .or_else(|| {
                    self.config
                        .bye_preferences
                        .get(team)
                        .map(|&[a, b]| [a.to_string(), b.to_string()])
                })
                .unwrap_or(["".into(), "".into()]);

            // Clone team name into the closures (required because the closure
            // outlives this loop iteration).
            let t1 = team.clone();
            let t2 = team.clone();

            rows = rows.push(
                row![
                    text(team.clone()).color(c).width(180),
                    text("1st:").size(13),
                    text_input("wk", &defaults[0])
                        .on_input(move |s| Message::PrefChanged(t1.clone(), 0, s))
                        .width(60),
                    text("2nd:").size(13),
                    text_input("wk", &defaults[1])
                        .on_input(move |s| Message::PrefChanged(t2.clone(), 1, s))
                        .width(60),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }

        scrollable(container(rows).padding(20).width(Length::Fill))
            .height(Length::Fill)
            .into()
    }

    /// Step 5 — Bye-restriction checkbox grid.
    ///
    /// Rendered as a table: rows are teams, columns are weeks.  A checked cell
    /// means that team CANNOT have its bye in that week (hard constraint).
    ///
    /// Column headers show "Wk N"; each cell is centred within a fixed-width
    /// container so the columns align regardless of week number width.
    fn view_bye_restrictions(&self) -> Element<'_, Message> {
        let mut sorted_weeks = self.config.weeks.clone();
        sorted_weeks.sort();

        // Build the column-header row (week labels).
        let mut header = row![Space::with_width(180)].spacing(0);
        for w in &sorted_weeks {
            header = header
                .push(container(text(format!("Wk {}", w)).size(13)).center_x(60));
        }

        let mut rows = column![
            text("Check weeks a team CANNOT have as their bye.").size(13),
            header,
        ]
        .spacing(8);

        // One row per team: coloured name + one checkbox per week.
        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            let restrictions = self
                .config
                .bye_restrictions
                .get(team)
                .cloned()
                .unwrap_or_default();

            let mut row_items = row![text(team.clone()).color(c).width(180)].spacing(0);

            for &w in &sorted_weeks {
                let is_checked = restrictions.contains(&w);
                let tc = team.clone();
                row_items = row_items.push(
                    container(
                        // The checkbox label is empty — the column header conveys
                        // the week number.  The `on_toggle` closure ignores the
                        // bool argument because the toggle is handled by the
                        // message arm which checks the existing list.
                        checkbox("", is_checked)
                            .on_toggle(move |_| Message::ToggleRestriction(tc.clone(), w)),
                    )
                    .center_x(60),
                );
            }

            rows = rows.push(row_items);
        }

        scrollable(container(rows).padding(20).width(Length::Fill))
            .height(Length::Fill)
            .into()
    }

    /// Step 6 — Score-exclusion checkboxes.
    ///
    /// Teams that are checked will not contribute to the fairness score.
    /// They still receive bye weeks — the exclusion only affects *ranking*
    /// of solutions, not which schedules are generated.
    fn view_score_exclusions(&self) -> Element<'_, Message> {
        let mut rows = column![
            text("Exclude teams from the fairness score entirely.").size(13),
            text("Checked teams will not affect which schedule ranks highest.").size(12),
        ]
        .spacing(10);

        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            let is_excluded = self.config.score_excluded.contains(team);
            let tc = team.clone();

            rows = rows.push(
                row![
                    checkbox("", is_excluded)
                        .on_toggle(move |_| Message::ToggleExclusion(tc.clone())),
                    text(team.clone()).color(c),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }

        scrollable(container(rows).padding(20).width(Length::Fill))
            .height(Length::Fill)
            .into()
    }

    /// Results screen — ranked schedule solutions.
    ///
    /// Layout:
    /// 1. Error panel (if the scheduler returned an error).
    /// 2. "No results" message (if no valid schedules were found).
    /// 3. Rank tab buttons to switch between up to 5 solutions.
    /// 4. Summary line: fairness score + hosting-streak penalty.
    /// 5. Bye-week table with score indicators (★★ / ★ / — / ⊘).
    /// 6. Week-by-week match listing with host and away teams coloured.
    fn view_results(&self) -> Element<'_, Message> {
        // ── Error / empty states ──────────────────────────────────────────────
        if let Some(e) = &self.run_error {
            return container(
                column![
                    text("Error:").size(16),
                    text(e).color(color!(0xff6666)),
                ]
                .spacing(10),
            )
            .padding(20)
            .into();
        }
        if self.results.is_empty() {
            return container(text("No valid schedules found.").size(16))
                .padding(20)
                .into();
        }

        // ── Rank tab buttons ──────────────────────────────────────────────────
        // Primary style for the active rank; secondary for all others.
        let tabs: Element<Message> = row(
            self.results
                .iter()
                .map(|sol| {
                    let r = sol.rank;
                    if r == self.selected_rank {
                        button(text(format!("Rank #{}", r)).size(13))
                            .on_press(Message::SelectRank(r))
                            .into()
                    } else {
                        button(text(format!("Rank #{}", r)).size(13))
                            .style(button::secondary)
                            .on_press(Message::SelectRank(r))
                            .into()
                    }
                })
                .collect::<Vec<_>>(),
        )
        .spacing(6)
        .into();

        // Look up the currently selected solution.
        let sol = match self.results.iter().find(|s| s.rank == self.selected_rank) {
            Some(s) => s,
            None => return text("Select a schedule.").into(),
        };

        let summary = text(format!(
            "Fairness Score: {}   |   Host Streak Penalty: {}",
            sol.score, sol.penalty
        ))
        .size(13);

        // ── Bye-week table ────────────────────────────────────────────────────
        let mut bye_col = column![text("Bye Weeks").size(15)].spacing(6);
        for team in &self.config.teams {
            let c    = team_color(&self.config.teams, team);
            let week = sol.bye_assignment.get(team).copied().unwrap_or(0);
            let sc   = sol.bye_detail.get(team).copied().unwrap_or(0);
            // Human-readable score indicator.
            let indicator = match sc {
                -1 => "⊘ (excluded)",
                2  => "★★ (1st pref)",
                1  => "★ (2nd pref)",
                _  => "— (no pref)",
            };
            bye_col = bye_col.push(
                row![
                    text(team.clone()).color(c).width(180),
                    text(format!("Week {}", week)).width(80),
                    text(indicator).size(13),
                ]
                .spacing(10),
            );
        }

        // ── Week-by-week match listing ────────────────────────────────────────
        let mut match_col = column![text("Match Schedule").size(15)].spacing(4);
        let mut sorted_weeks = self.config.weeks.clone();
        sorted_weeks.sort();

        for w in &sorted_weeks {
            match_col = match_col.push(text(format!("Week {}", w)).size(14));

            // List every game this week with coloured team names.
            if let Some(games) = sol.schedule.get(w) {
                for (host, away) in games {
                    let hc = team_color(&self.config.teams, host);
                    let ac = team_color(&self.config.teams, away);
                    match_col = match_col.push(
                        row![
                            Space::with_width(16),
                            text(host).color(hc).size(13),
                            text(" hosts ").size(13),
                            text(away).color(ac).size(13),
                        ],
                    );
                }
            }

            // Show which team has the bye this week (if any).
            if let Some((team, _)) = sol.bye_assignment.iter().find(|(_, &wk)| wk == *w) {
                let c = team_color(&self.config.teams, team);
                match_col = match_col.push(
                    row![
                        Space::with_width(16),
                        text(team).color(c).size(13),
                        text(" — BYE").size(13),
                    ],
                );
            }

            // Vertical breathing room between weeks.
            match_col = match_col.push(Space::with_height(6));
        }

        scrollable(
            container(
                column![
                    tabs,
                    summary,
                    horizontal_rule(1),
                    bye_col,
                    horizontal_rule(1),
                    match_col,
                ]
                .spacing(16),
            )
            .padding(20)
            .width(Length::Fill),
        )
        .height(Length::Fill)
        .into()
    }
}
