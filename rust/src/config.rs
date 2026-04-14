// ── Persisted config ──────────────────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use std::collections::{HashMap};

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