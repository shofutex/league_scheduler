//! Swim League Scheduler — iced 0.13
//! Wizard: Teams → Weeks → Base Schedule → Bye Preferences → Bye Restrictions → Results

use iced::widget::{
    button, checkbox, column, container, horizontal_rule, row, scrollable, text, text_input, Space,
};
use iced::{color, Alignment, Color, Element, Length, Task, Theme};
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

fn main() -> iced::Result {
    iced::application("Swim League Scheduler", SwimScheduler::update, SwimScheduler::view)
        .theme(|_| Theme::TokyoNight)
        .window_size((900.0, 700.0))
        .run_with(SwimScheduler::new)
}

// ── Persisted config ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LeagueConfig {
    teams: Vec<String>,
    weeks: Vec<u32>,
    labels: Vec<String>,
    base_schedule: HashMap<u32, Vec<[String; 2]>>,
    bye_preferences: HashMap<String, [u32; 2]>,
    bye_restrictions: HashMap<String, Vec<u32>>,
    #[serde(default)]
    score_excluded: Vec<String>,
}

impl Default for LeagueConfig {
    fn default() -> Self {
        Self {
            teams: vec![],
            weeks: vec![1, 2, 3, 4, 5],
            labels: vec!["A","B","C","D","E"].into_iter().map(String::from).collect(),
            base_schedule: default_5team_schedule(),
            bye_preferences: HashMap::new(),
            bye_restrictions: HashMap::new(),
            score_excluded: vec![],
        }
    }
}

fn default_5team_schedule() -> HashMap<u32, Vec<[String; 2]>> {
    let mut m = HashMap::new();
    m.insert(1, vec![["A","B"],["C","D"]]);
    m.insert(2, vec![["A","C"],["D","E"]]);
    m.insert(3, vec![["A","D"],["B","E"]]);
    m.insert(4, vec![["A","E"],["B","C"]]);
    m.insert(5, vec![["B","D"],["C","E"]]);
    m.into_iter().map(|(k,v)| (k, v.into_iter().map(|[a,b]| [a.into(),b.into()]).collect())).collect()
}

fn default_6team_schedule() -> HashMap<u32, Vec<[String; 2]>> {
    let mut m: HashMap<u32, Vec<[&str;2]>> = HashMap::new();
    m.insert(1, vec![["A","B"],["C","F"],["D","E"]]);
    m.insert(2, vec![["A","C"],["B","E"],["D","F"]]);
    m.insert(3, vec![["A","D"],["B","F"],["C","E"]]);
    m.insert(4, vec![["A","E"],["B","C"],["D","F"]]);
    m.insert(5, vec![["A","F"],["B","D"],["C","E"]]);
    m.into_iter().map(|(k,v)| (k, v.into_iter().map(|[a,b]| [a.into(),b.into()]).collect())).collect()
}

// ── Scheduling ────────────────────────────────────────────────────────────────

type Matchup = (String, String);
type Schedule = HashMap<u32, Vec<Matchup>>;
type ByeAssignment = HashMap<String, u32>;

#[derive(Debug, Clone)]
struct Solution {
    rank: usize,
    score: i32,
    penalty: i32,
    bye_detail: HashMap<String, i32>,
    bye_assignment: ByeAssignment,
    schedule: Schedule,
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
    let mut options = Vec::new();
    for bits in 0u64..(1u64 << n) {
        let mut counts: HashMap<&str,u32> = HashMap::new();
        let mut sched: Schedule = weeks.iter().map(|&w| (w,vec![])).collect();
        for (i,(w,(t1,t2))) in games.iter().enumerate() {
            let (host,away) = if (bits>>i)&1==0 { (t1.as_str(),t2.as_str()) } else { (t2.as_str(),t1.as_str()) };
            *counts.entry(host).or_insert(0) += 1;
            sched.get_mut(w).unwrap().push((host.to_string(),away.to_string()));
        }

        // If we have a 5-league team, then we can 2 hosts per team
        if teams.len() == 5 {
            if teams.iter().all(|t| counts.get(t.as_str()).copied().unwrap_or(0)==2) {
                options.push(sched);
            }
        }
        else { // otherwise, we have at least 2
            if teams.iter().all(|t| counts.get(t.as_str()).copied().unwrap_or(0)>=2) {
                options.push(sched);
            }
        }
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

fn run_scheduler(config: &LeagueConfig) -> Result<Vec<Solution>, String> {
    let teams = &config.teams;
    let weeks = &config.weeks;
    let labels = &config.labels;

    eprintln!("Starting the schedule generation...");

    if teams.len() < 2 { return Err("Need at least 2 teams.".into()); }
    if weeks.is_empty() { return Err("Need at least one week.".into()); }
    if labels.len() != teams.len() {
        return Err(format!("Labels count ({}) must match team count ({}).", labels.len(), teams.len()));
    }
    let mut solutions: Vec<Solution> = Vec::new();
    let mut seen: HashSet<Vec<(u32,Vec<(String,String)>)>> = HashSet::new();
    let team_set: HashSet<&str> = teams.iter().map(|s|s.as_str()).collect();

    eprintln!("Entering permutations step");

    let mut numPerms = 0;

    let totalPerms = (teams.iter().permutations(teams.len())).count();

    eprintln!("Total number of permutations: {totalPerms}");
    
    for perm in teams.iter().permutations(teams.len()) {
        eprintln!("Permutation {numPerms}");
        numPerms = numPerms + 1;
        
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

// ── Team colors ───────────────────────────────────────────────────────────────

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

fn team_color(teams: &[String], team: &str) -> Color {
    let idx = teams.iter().position(|t| t == team).unwrap_or(0);
    TEAM_COLORS[idx % TEAM_COLORS.len()]
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

// ── Messages ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Message {
    GoTo(Step), Next, Back,
    NewTeamNameChanged(String), AddTeam, RemoveTeam(usize),
    SaveConfig, LoadConfig,
    ConfigSaved(Result<(), String>), ConfigLoaded(Result<LeagueConfig, String>),
    WeeksInputChanged(String), ApplyWeeks,
    BaseScheduleInputChanged(u32, String), ApplyBaseSchedule,
    UseDefault5Team, UseDefault6Team,
    PrefChanged(String, usize, String),
    ToggleRestriction(String, u32),
    ToggleExclusion(String),
    RunScheduler, SelectRank(usize),
    ExportResults, ExportDone(Result<(), String>),
}

// ── Update ────────────────────────────────────────────────────────────────────

impl SwimScheduler {
    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::GoTo(s) => self.step = Some(s),
            Message::Next => {
                let steps = Step::all();
                if let Some(pos) = steps.iter().position(|s| s == self.current_step()) {
                    if pos + 1 < steps.len() { self.step = Some(steps[pos+1].clone()); }
                }
            }
            Message::Back => {
                let steps = Step::all();
                if let Some(pos) = steps.iter().position(|s| s == self.current_step()) {
                    if pos > 0 { self.step = Some(steps[pos-1].clone()); }
                }
            }

            Message::NewTeamNameChanged(s) => self.new_team_name = s,
            Message::AddTeam => {
                let name = self.new_team_name.trim().to_string();
                if !name.is_empty() && !self.config.teams.contains(&name) {
                    self.config.teams.push(name);
                    self.new_team_name.clear();
                    self.sync_labels();
                }
            }
            Message::RemoveTeam(idx) => {
                if idx < self.config.teams.len() {
                    self.config.teams.remove(idx);
                    self.sync_labels();
                }
            }

            Message::SaveConfig => {
                let config = self.config.clone();
                return Task::perform(
                    async move {
                        let path = rfd::AsyncFileDialog::new().set_file_name("league.json").add_filter("JSON",&["json"]).save_file().await;
                        match path {
                            None => Err("Cancelled".into()),
                            Some(p) => {
                                let json = serde_json::to_string_pretty(&config).map_err(|e| e.to_string())?;
                                std::fs::write(p.path(), json).map_err(|e| e.to_string())
                            }
                        }
                    },
                    Message::ConfigSaved,
                );
            }
            Message::LoadConfig => {
                return Task::perform(
                    async {
                        let path = rfd::AsyncFileDialog::new().add_filter("JSON",&["json"]).pick_file().await;
                        match path {
                            None => Err("Cancelled".into()),
                            Some(p) => {
                                let bytes = std::fs::read(p.path()).map_err(|e| e.to_string())?;
                                serde_json::from_slice::<LeagueConfig>(&bytes).map_err(|e| e.to_string())
                            }
                        }
                    },
                    Message::ConfigLoaded,
                );
            }
            Message::ConfigSaved(_) => {}
            Message::ConfigLoaded(Ok(cfg)) => {
                self.config = cfg;
                self.weeks_input = self.config.weeks.iter().map(|w| w.to_string()).collect::<Vec<_>>().join(", ");
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.pref_inputs = self.config.bye_preferences.iter()
                    .map(|(k,&[a,b])| (k.clone(),[a.to_string(),b.to_string()])).collect();
            }
            Message::ConfigLoaded(Err(_)) => {}

            Message::WeeksInputChanged(s) => self.weeks_input = s,
            Message::ApplyWeeks => {
                let parsed: Result<Vec<u32>,_> = self.weeks_input.split(',').map(|s| s.trim().parse::<u32>()).collect();
                match parsed {
                    Ok(w) if !w.is_empty() => { self.weeks_error=None; self.config.weeks=w; self.sync_base_schedule_inputs(); }
                    _ => self.weeks_error = Some("Enter comma-separated week numbers, e.g. 1, 2, 3, 4, 5".into()),
                }
            }

            Message::BaseScheduleInputChanged(w, s) => { self.base_schedule_inputs.insert(w, s); }
            Message::ApplyBaseSchedule => {
                match self.parse_base_schedule() {
                    Ok(bs) => { self.config.base_schedule=bs; self.base_schedule_error=None; }
                    Err(e) => self.base_schedule_error=Some(e),
                }
            }
            Message::UseDefault5Team => {
                self.config.labels = ["A","B","C","D","E"].iter().map(|s| s.to_string()).collect();
                self.config.base_schedule = default_5team_schedule();
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.base_schedule_error = None;
            }
            Message::UseDefault6Team => {
                self.config.labels = ["A","B","C","D","E","F"].iter().map(|s| s.to_string()).collect();
                self.config.base_schedule = default_6team_schedule();
                self.base_schedule_inputs = schedule_to_inputs(&self.config.base_schedule);
                self.base_schedule_error = None;
            }

            Message::PrefChanged(team, idx, val) => {
                let entry = self.pref_inputs.entry(team.clone()).or_insert(["".into(),"".into()]);
                entry[idx] = val;
                let a = entry[0].trim().parse::<u32>();
                let b = entry[1].trim().parse::<u32>();
                if let (Ok(f),Ok(s)) = (a,b) { self.config.bye_preferences.insert(team,[f,s]); }
            }

            Message::ToggleRestriction(team, week) => {
                let list = self.config.bye_restrictions.entry(team).or_default();
                if let Some(pos) = list.iter().position(|&w| w==week) { list.remove(pos); }
                else { list.push(week); }
            }
            Message::ToggleExclusion(team) => {
                if let Some(pos) = self.config.score_excluded.iter().position(|t| t==&team) {
                    self.config.score_excluded.remove(pos);
                } else {
                    self.config.score_excluded.push(team);
                }
            }

            Message::RunScheduler => {
                self.step = Some(Step::Results);
                match run_scheduler(&self.config) {
                    Ok(sols) => { self.results=sols; self.selected_rank=1; self.run_error=None; }
                    Err(e) => { self.results.clear(); self.run_error=Some(e); }
                }
            }
            Message::SelectRank(r) => self.selected_rank = r,

            Message::ExportResults => {
                let results = self.results.clone();
                let weeks = self.config.weeks.clone();
                let teams = self.config.teams.clone();
                return Task::perform(
                    async move {
                        let path = rfd::AsyncFileDialog::new().set_file_name("schedules.txt").add_filter("Text",&["txt"]).save_file().await;
                        match path {
                            None => Err("Cancelled".into()),
                            Some(p) => {
                                let mut out = String::new();
                                for sol in &results {
                                    out.push_str(&format!("\n==============================\nRank: {}\nScore: {}\n\nBye Weeks\n", sol.rank, sol.score));
                                    for t in &teams {
                                        out.push_str(&format!("  {}: week {}  (score {})\n", t,
                                            sol.bye_assignment.get(t).copied().unwrap_or(0),
                                            sol.bye_detail.get(t).copied().unwrap_or(0)));
                                    }
                                    out.push_str("\nMatches\n");
                                    let mut sw = weeks.clone(); sw.sort();
                                    for w in &sw {
                                        out.push_str(&format!("\n  Week {}\n", w));
                                        if let Some(games) = sol.schedule.get(w) {
                                            for (h,a) in games { out.push_str(&format!("    {} hosts {}\n", h, a)); }
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
            Message::ExportDone(_) => {}
        }
        Task::none()
    }
}

// ── View ──────────────────────────────────────────────────────────────────────

impl SwimScheduler {
    fn view(&self) -> Element<Message> {
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

    fn view_header(&self) -> Element<Message> {
        container(
            column![
                text("🏊 Swim League Scheduler").size(28),
                text(self.current_step().title()).size(14),
            ].spacing(4)
        ).padding(20).into()
    }

    fn view_breadcrumb(&self) -> Element<Message> {
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

    fn view_nav(&self) -> Element<Message> {
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
                button(text("📂 Load")).style(button::secondary).on_press(Message::LoadConfig),
                button(text("💾 Save")).style(button::secondary).on_press(Message::SaveConfig),
                right,
            ].spacing(10).align_y(Alignment::Center)
        ).padding([12,20]).into()
    }

    fn view_teams(&self) -> Element<Message> {
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

    fn view_weeks(&self) -> Element<Message> {
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

    fn view_base_schedule(&self) -> Element<Message> {
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

    fn view_bye_preferences(&self) -> Element<Message> {
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

    fn view_bye_restrictions(&self) -> Element<Message> {
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


    fn view_score_exclusions(&self) -> Element<Message> {
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

    fn view_results(&self) -> Element<Message> {
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
