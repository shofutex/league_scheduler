
use crate::config::LeagueConfig;

use crate::config::schedule_to_inputs;

use crate::scheduler::Solution;

use crate::message::Message;

use std::collections::{HashMap};

use iced::Task;


#[derive(Debug, Clone, PartialEq)]
pub enum Step { Teams, Weeks, BaseSchedule, ByePreferences, ByeRestrictions, ScoreExclusions, Results }

impl Step {
    pub fn title(&self) -> &'static str {
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
    pub fn all() -> &'static [Step] {
        &[Step::Teams, Step::Weeks, Step::BaseSchedule, Step::ByePreferences, Step::ByeRestrictions, Step::ScoreExclusions, Step::Results]
    }
}

#[derive(Debug, Default)]
pub struct SwimScheduler {
    pub config: LeagueConfig,
    pub step: Option<Step>,
    pub new_team_name: String,
    pub weeks_input: String,
    pub weeks_error: Option<String>,
    pub base_schedule_inputs: HashMap<u32, String>,
    pub base_schedule_error: Option<String>,
    pub pref_inputs: HashMap<String, [String; 2]>,
    pub results: Vec<Solution>,
    pub selected_rank: usize,
    pub run_error: Option<String>,
    // Progress modal state
    pub is_running: bool,
    pub scheduler_progress: f32,
}


impl SwimScheduler {
    pub fn new() -> (Self, Task<Message>) {
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

    pub fn current_step(&self) -> &Step {
        self.step.as_ref().unwrap_or(&Step::Teams)
    }
}
