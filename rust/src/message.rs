// ── Messages ──────────────────────────────────────────────────────────────────
use crate::config::LeagueConfig;
use crate::state::Step;

#[derive(Debug, Clone)]
pub enum Message {
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