// ── Persisted config ──────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::{HashMap};
use iced::{Color};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeagueConfig {
    pub teams: Vec<String>,
    pub weeks: Vec<u32>,
    pub labels: Vec<String>,
    pub base_schedule: HashMap<u32, Vec<[String; 2]>>,
    pub bye_preferences: HashMap<String, [u32; 2]>,
    pub bye_restrictions: HashMap<String, Vec<u32>>,
    #[serde(default)]
    pub score_excluded: Vec<String>,
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

pub fn default_5team_schedule() -> HashMap<u32, Vec<[String; 2]>> {
    let mut m = HashMap::new();
    m.insert(1, vec![["A","B"],["C","D"]]);
    m.insert(2, vec![["A","C"],["D","E"]]);
    m.insert(3, vec![["A","D"],["B","E"]]);
    m.insert(4, vec![["A","E"],["B","C"]]);
    m.insert(5, vec![["B","D"],["C","E"]]);
    m.into_iter().map(|(k,v)| (k, v.into_iter().map(|[a,b]| [a.into(),b.into()]).collect())).collect()
}

pub fn default_6team_schedule() -> HashMap<u32, Vec<[String; 2]>> {
    let mut m: HashMap<u32, Vec<[&str;2]>> = HashMap::new();
    m.insert(1, vec![["A","B"],["C","F"],["D","E"]]);
    m.insert(2, vec![["A","C"],["B","E"],["D","F"]]);
    m.insert(3, vec![["A","D"],["B","F"],["C","E"]]);
    m.insert(4, vec![["A","E"],["B","C"],["D","F"]]);
    m.insert(5, vec![["A","F"],["B","D"],["C","E"]]);
    m.into_iter().map(|(k,v)| (k, v.into_iter().map(|[a,b]| [a.into(),b.into()]).collect())).collect()
}

pub fn schedule_to_inputs(bs: &HashMap<u32, Vec<[String;2]>>) -> HashMap<u32, String> {
    bs.iter().map(|(&w, games)| {
        (w, games.iter().map(|[a,b]| format!("{} vs {}", a, b)).collect::<Vec<_>>().join(", "))
    }).collect()
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

pub fn team_color(teams: &[String], team: &str) -> Color {
    let idx = teams.iter().position(|t| t == team).unwrap_or(0);
    TEAM_COLORS[idx % TEAM_COLORS.len()]
}