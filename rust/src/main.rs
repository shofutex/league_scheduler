//! Swim League Scheduler — iced 0.13
//! Wizard: Teams → Weeks → Base Schedule → Bye Preferences → Bye Restrictions → Results

mod config;
mod scheduler;
mod message;
mod update;

// Make sure we can get the items we need from config.rs
use crate::config::LeagueConfig;
use crate::config::team_color;

use crate::scheduler::Solution;

use crate::message::Message;

use iced::widget::{
    button, checkbox, column, container, horizontal_rule, row, scrollable, text, text_input, Space,
};
use iced::{color, Alignment, Element, Length, Task, Theme};
use std::collections::{HashMap};

static INTER: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");

fn main() -> iced::Result {
    iced::application("Swim League Scheduler", SwimScheduler::update, SwimScheduler::view)
        .theme(|_| Theme::TokyoNight)
        .window_size((900.0, 700.0))
        .font(INTER)
        .default_font(iced::Font::with_name("Inter"))
        .run_with(SwimScheduler::new)
}







// ── Wizard steps ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Step { Teams, Weeks, BaseSchedule, ByePreferences, ByeRestrictions, ScoreExclusions, Results }

impl Step {
    fn title(&self) -> &'static str {
        match self {
            Step::Teams => "Step 1 — Teams",
            Step::Weeks => "Step 2 — Weeks",
            Step::BaseSchedule => "Step 3 — Base Schedule",
            Step::ByePreferences => "Step 4 — Bye Preferences",
            Step::ByeRestrictions => "Step 5 — Bye Restrictions",
            Step::ScoreExclusions => "Step 6 — Score Exclusions",
            Step::Results => "Results",
        }
    }
    fn all() -> &'static [Step] {
        &[Step::Teams, Step::Weeks, Step::BaseSchedule, Step::ByePreferences, Step::ByeRestrictions, Step::ScoreExclusions, Step::Results]
    }
}

// ── App state ─────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct SwimScheduler {
    config: LeagueConfig,
    step: Option<Step>,
    new_team_name: String,
    weeks_input: String,
    weeks_error: Option<String>,
    base_schedule_inputs: HashMap<u32, String>,
    base_schedule_error: Option<String>,
    pref_inputs: HashMap<String, [String; 2]>,
    results: Vec<Solution>,
    selected_rank: usize,
    run_error: Option<String>,
}

impl SwimScheduler {
    fn new() -> (Self, Task<Message>) {
        let config = LeagueConfig::default();
        let weeks_input = config.weeks.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(", ");
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

    fn current_step(&self) -> &Step {
        self.step.as_ref().unwrap_or(&Step::Teams)
    }
}

fn schedule_to_inputs(bs: &HashMap<u32, Vec<[String;2]>>) -> HashMap<u32, String> {
    bs.iter().map(|(&w, games)| {
        (w, games.iter().map(|[a,b]| format!("{} vs {}", a, b)).collect::<Vec<_>>().join(", "))
    }).collect()
}




// ── View ──────────────────────────────────────────────────────────────────────

impl SwimScheduler {
    fn view(&self) -> Element<'_, Message> {
        let content: Element<Message> = match self.current_step() {
            Step::Teams => self.view_teams(),
            Step::Weeks => self.view_weeks(),
            Step::BaseSchedule => self.view_base_schedule(),
            Step::ByePreferences => self.view_bye_preferences(),
            Step::ByeRestrictions => self.view_bye_restrictions(),
            Step::ScoreExclusions => self.view_score_exclusions(),
            Step::Results => self.view_results(),
        };

        column![
            self.view_header(),
            self.view_breadcrumb(),
            horizontal_rule(1),
            content,
            horizontal_rule(1),
            self.view_nav(),
        ]
        .into()
    }

    fn view_header(&self) -> Element<'_, Message> {
        container(
            column![
                text("Swim League Scheduler").size(28),
                text(self.current_step().title()).size(14),
            ].spacing(4)
        ).padding(20).into()
    }

    fn view_breadcrumb(&self) -> Element<'_, Message> {
        let current = self.current_step();
        let items: Vec<Element<Message>> = Step::all().iter().enumerate().map(|(i, s)| {
            let label = match s {
                Step::Teams => "Teams", Step::Weeks => "Weeks",
                Step::BaseSchedule => "Schedule", Step::ByePreferences => "Prefs",
                Step::ByeRestrictions => "Restrictions", Step::ScoreExclusions => "Exclusions", Step::Results => "Results",
            };
            let btn: Element<Message> = if s == current {
                button(text(label).size(13)).on_press(Message::GoTo(s.clone())).into()
            } else {
                button(text(label).size(13)).style(button::secondary).on_press(Message::GoTo(s.clone())).into()
            };
            if i < Step::all().len() - 1 {
                row![btn, text(" › ").size(13)].align_y(Alignment::Center).into()
            } else { btn }
        }).collect();
        container(row(items).spacing(4).align_y(Alignment::Center)).padding([6,20]).into()
    }

    fn view_nav(&self) -> Element<'_, Message> {
        let steps = Step::all();
        let pos = steps.iter().position(|s| s == self.current_step()).unwrap_or(0);

        let back: Element<Message> = if pos > 0 {
            button(text("← Back")).style(button::secondary).on_press(Message::Back).into()
        } else {
            button(text("← Back")).style(button::secondary).into()
        };

        let right: Element<Message> = match self.current_step() {
            Step::Results => button(text("Export Results")).on_press(Message::ExportResults).into(),
            Step::ScoreExclusions => button(text("Run Scheduler →")).on_press(Message::RunScheduler).into(),
            Step::ByeRestrictions => button(text("Next →")).on_press(Message::Next).into(),
            _ => button(text("Next →")).on_press(Message::Next).into(),
        };

        container(
            row![
                back,
                Space::with_width(Length::Fill),
                button(text("⬒ Load")).style(button::secondary).on_press(Message::LoadConfig),
                button(text("↓ Save")).style(button::secondary).on_press(Message::SaveConfig),
                right,
            ].spacing(10).align_y(Alignment::Center)
        ).padding([12,20]).into()
    }

    fn view_teams(&self) -> Element<'_, Message> {
        let mut list = column![text("Teams").size(16)].spacing(8);
        for (i, team) in self.config.teams.iter().enumerate() {
            let c = team_color(&self.config.teams, team);
            list = list.push(
                row![
                    text(format!("{}. {}", i+1, team)).color(c).width(Length::Fill),
                    button(text("Remove")).style(button::danger).on_press(Message::RemoveTeam(i)),
                ].spacing(10).align_y(Alignment::Center)
            );
        }
        let add_row = row![
            text_input("New team name…", &self.new_team_name)
                .on_input(Message::NewTeamNameChanged)
                .on_submit(Message::AddTeam)
                .width(300),
            button(text("Add Team")).on_press(Message::AddTeam),
        ].spacing(10).align_y(Alignment::Center);

        scrollable(
            container(column![list, add_row,
                text(format!("{} team(s) added. Supports 5 or 6 teams.", self.config.teams.len())).size(12)
            ].spacing(20)).padding(20).width(Length::Fill)
        ).height(Length::Fill).into()
    }

    fn view_weeks(&self) -> Element<'_, Message> {
        let err: Element<Message> = if let Some(e) = &self.weeks_error {
            text(e).color(color!(0xff6666)).into()
        } else { Space::with_height(0).into() };

        scrollable(container(column![
            text("Enter week numbers separated by commas:").size(16),
            row![
                text_input("e.g. 1, 2, 3, 4, 5", &self.weeks_input)
                    .on_input(Message::WeeksInputChanged)
                    .on_submit(Message::ApplyWeeks)
                    .width(300),
                button(text("Apply")).on_press(Message::ApplyWeeks),
            ].spacing(10).align_y(Alignment::Center),
            err,
            text(format!("Weeks: {}", self.config.weeks.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(", "))).size(13),
        ].spacing(16)).padding(20).width(Length::Fill)).height(Length::Fill).into()
    }

    fn view_base_schedule(&self) -> Element<'_, Message> {
        let mut sorted_weeks = self.config.weeks.clone(); sorted_weeks.sort();
        let mut weeks_col = column![].spacing(10);
        for w in &sorted_weeks {
            let val = self.base_schedule_inputs.get(w).cloned().unwrap_or_default();
            let week = *w;
            weeks_col = weeks_col.push(
                row![
                    text(format!("Week {}:", w)).width(70),
                    text_input("A vs B, C vs D", &val)
                        .on_input(move |s| Message::BaseScheduleInputChanged(week, s))
                        .width(Length::Fill),
                ].spacing(10).align_y(Alignment::Center)
            );
        }
        let err: Element<Message> = if let Some(e) = &self.base_schedule_error {
            text(e).color(color!(0xff6666)).into()
        } else { Space::with_height(0).into() };

        scrollable(container(column![
            text("For each week, enter matchups like: A vs B, C vs D\nOne team per week sits out (bye).").size(13),
            row![
                button(text("Load 5-Team Default")).style(button::secondary).on_press(Message::UseDefault5Team),
                button(text("Load 6-Team Default")).style(button::secondary).on_press(Message::UseDefault6Team),
            ].spacing(10),
            weeks_col,
            button(text("Apply Schedule")).on_press(Message::ApplyBaseSchedule),
            err,
            text(format!("Labels: {}", self.config.labels.join(", "))).size(12),
        ].spacing(16)).padding(20).width(Length::Fill)).height(Length::Fill).into()
    }

    fn view_bye_preferences(&self) -> Element<'_, Message> {
        let mut rows = column![
            text("Enter each team's 1st and 2nd preferred bye week. Leave blank for no preference.").size(13)
        ].spacing(10);

        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            let defaults = self.pref_inputs.get(team).cloned()
                .or_else(|| self.config.bye_preferences.get(team).map(|&[a,b]| [a.to_string(),b.to_string()]))
                .unwrap_or(["".into(),"".into()]);
            let t1 = team.clone(); let t2 = team.clone();
            rows = rows.push(
                row![
                    text(team.clone()).color(c).width(180),
                    text("1st:").size(13),
                    text_input("wk", &defaults[0]).on_input(move |s| Message::PrefChanged(t1.clone(),0,s)).width(60),
                    text("2nd:").size(13),
                    text_input("wk", &defaults[1]).on_input(move |s| Message::PrefChanged(t2.clone(),1,s)).width(60),
                ].spacing(8).align_y(Alignment::Center)
            );
        }
        scrollable(container(rows).padding(20).width(Length::Fill)).height(Length::Fill).into()
    }

    fn view_bye_restrictions(&self) -> Element<'_, Message> {
        let mut sorted_weeks = self.config.weeks.clone(); sorted_weeks.sort();

        let mut header = row![Space::with_width(180)].spacing(0);
        for w in &sorted_weeks {
            header = header.push(container(text(format!("Wk {}", w)).size(13)).center_x(60));
        }

        let mut rows = column![
            text("Check weeks a team CANNOT have as their bye.").size(13),
            header,
        ].spacing(8);

        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            let restrictions = self.config.bye_restrictions.get(team).cloned().unwrap_or_default();
            let mut row_items = row![text(team.clone()).color(c).width(180)].spacing(0);
            for &w in &sorted_weeks {
                let is_checked = restrictions.contains(&w);
                let tc = team.clone();
                row_items = row_items.push(
                    container(checkbox("", is_checked).on_toggle(move |_| Message::ToggleRestriction(tc.clone(), w))).center_x(60)
                );
            }
            rows = rows.push(row_items);
        }
        scrollable(container(rows).padding(20).width(Length::Fill)).height(Length::Fill).into()
    }


    fn view_score_exclusions(&self) -> Element<'_, Message> {
        let mut rows = column![
            text("Exclude teams from the fairness score entirely.").size(13),
            text("Checked teams will not affect which schedule ranks highest.").size(12),
        ].spacing(10);

        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            let is_excluded = self.config.score_excluded.contains(team);
            let tc = team.clone();
            rows = rows.push(
                row![
                    checkbox("", is_excluded).on_toggle(move |_| Message::ToggleExclusion(tc.clone())),
                    text(team.clone()).color(c),
                ].spacing(10).align_y(Alignment::Center)
            );
        }

        scrollable(container(rows).padding(20).width(Length::Fill)).height(Length::Fill).into()
    }

    fn view_results(&self) -> Element<'_, Message> {
        if let Some(e) = &self.run_error {
            return container(column![text("Error:").size(16), text(e).color(color!(0xff6666))].spacing(10)).padding(20).into();
        }
        if self.results.is_empty() {
            return container(text("No valid schedules found.").size(16)).padding(20).into();
        }

        // Rank tabs
        let tabs: Element<Message> = row(
            self.results.iter().map(|sol| {
                let r = sol.rank;
                if r == self.selected_rank {
                    button(text(format!("Rank #{}", r)).size(13)).on_press(Message::SelectRank(r)).into()
                } else {
                    button(text(format!("Rank #{}", r)).size(13)).style(button::secondary).on_press(Message::SelectRank(r)).into()
                }
            }).collect::<Vec<_>>()
        ).spacing(6).into();

        let sol = match self.results.iter().find(|s| s.rank == self.selected_rank) {
            Some(s) => s,
            None => return text("Select a schedule.").into(),
        };

        let summary = text(format!("Fairness Score: {}   |   Host Streak Penalty: {}", sol.score, sol.penalty)).size(13);

        // Bye weeks
        let mut bye_col = column![text("Bye Weeks").size(15)].spacing(6);
        for team in &self.config.teams {
            let c = team_color(&self.config.teams, team);
            let week = sol.bye_assignment.get(team).copied().unwrap_or(0);
            let score = sol.bye_detail.get(team).copied().unwrap_or(0);
            let indicator = match score { -1 => "⊘ (excluded)", 2 => "★★ (1st pref)", 1 => "★ (2nd pref)", _ => "— (no pref)" };
            bye_col = bye_col.push(
                row![
                    text(team.clone()).color(c).width(180),
                    text(format!("Week {}", week)).width(80),
                    text(indicator).size(13),
                ].spacing(10)
            );
        }

        // Match schedule
        let mut match_col = column![text("Match Schedule").size(15)].spacing(4);
        let mut sorted_weeks = self.config.weeks.clone(); sorted_weeks.sort();
        for w in &sorted_weeks {
            match_col = match_col.push(text(format!("Week {}", w)).size(14));
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
                        ]
                    );
                }
            }
            if let Some((team, _)) = sol.bye_assignment.iter().find(|(_,&wk)| wk == *w) {
                let c = team_color(&self.config.teams, team);
                match_col = match_col.push(
                    row![Space::with_width(16), text(team).color(c).size(13), text(" — BYE").size(13)]
                );
            }
            match_col = match_col.push(Space::with_height(6));
        }

        scrollable(
            container(
                column![tabs, summary, horizontal_rule(1), bye_col, horizontal_rule(1), match_col].spacing(16)
            ).padding(20).width(Length::Fill)
        ).height(Length::Fill).into()
    }
}

// ── Sync helpers ──────────────────────────────────────────────────────────────

impl SwimScheduler {
    fn sync_labels(&mut self) {
        let chars = ['A','B','C','D','E','F','G','H'];
        self.config.labels = chars[..self.config.teams.len().min(chars.len())].iter().map(|c| c.to_string()).collect();
    }
    fn sync_base_schedule_inputs(&mut self) {
        self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
    }
    fn parse_base_schedule(&self) -> Result<HashMap<u32, Vec<[String;2]>>, String> {
        let mut result = HashMap::new();
        for (&w, raw) in &self.base_schedule_inputs {
            let matchups: Result<Vec<[String;2]>,String> = raw.split(',')
                .filter(|s| !s.trim().is_empty())
                .map(|m| {
                    let parts: Vec<&str> = m.split("vs").map(|s| s.trim()).collect();
                    if parts.len()!=2 || parts[0].is_empty() || parts[1].is_empty() {
                        Err(format!("Invalid matchup '{}' in week {}", m.trim(), w))
                    } else { Ok([parts[0].to_string(), parts[1].to_string()]) }
                }).collect();
            result.insert(w, matchups?);
        }
        Ok(result)
    }
}
