// ── Update ────────────────────────────────────────────────────────────────────
use iced::Task;

use crate::SwimScheduler;
use crate::message::Message;
use crate::Step;
use crate::schedule_to_inputs;
use crate::scheduler::run_scheduler;


use crate::config::LeagueConfig;
use crate::config::default_5team_schedule;
use crate::config::default_6team_schedule;


impl SwimScheduler {
    pub fn update(&mut self, message: Message) -> Task<Message> {
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
