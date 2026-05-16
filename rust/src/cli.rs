//! # cli.rs
//!
//! Headless command-line interface for the Swim League Scheduler.
//!
//! When the binary is invoked with a path argument it skips the iced GUI
//! entirely, loads the `LeagueConfig` from the given JSON file, runs the
//! combinatorial scheduler, and prints the ranked results to stdout in the
//! same human-readable format used by the GUI's Export feature.
//!
//! ## Usage
//!
//! ```text
//! swim-scheduler [<config.json>]
//! ```
//!
//! - **No arguments** → launch the normal GUI (existing behaviour).
//! - **One argument** → headless CLI mode.
//!   - `--help` / `-h` → print usage and exit successfully.
//!   - Any other value → treated as a path to a `LeagueConfig` JSON file.
//!
//! ## Exit codes
//!
//! | Code | Meaning                                         |
//! |------|-------------------------------------------------|
//! | 0    | Success (results printed) or `--help` shown     |
//! | 1    | File not found, JSON parse error, or no valid   |
//! |      | schedules produced by the scheduler             |

use std::path::Path;

use crate::config::LeagueConfig;
use crate::scheduler::{run_scheduler_with_progress, Solution};

// ── Entry point ───────────────────────────────────────────────────────────────

/// Inspect `std::env::args` and decide whether to run in CLI mode.
///
/// Returns `Some(exit_code)` if the program should exit after this call
/// (CLI mode was detected and either succeeded or failed).
/// Returns `None` if no CLI argument was supplied and the GUI should start
/// normally.
///
/// Keeping the decision logic here — rather than inside `main` — makes it
/// straightforward to unit-test without spawning a subprocess.
pub fn run_if_cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(String::as_str) {
        // No argument: launch the GUI.
        None => None,

        // Help flags: print usage and exit cleanly.
        Some("--help") | Some("-h") => {
            print_usage(&args[0]);
            Some(0)
        }

        // Anything else is treated as a file path.
        Some(path) => Some(run_cli(path)),
    }
}

// ── CLI runner ────────────────────────────────────────────────────────────────

/// Load `path`, run the scheduler, and print results.
/// Returns 0 on success, 1 on any error.
fn run_cli(path: &str) -> i32 {
    // ── Load & parse the JSON config ─────────────────────────────────────────
    let config = match load_config(path) {
        Ok(c)  => c,
        Err(e) => {
            eprintln!("Error loading '{}': {}", path, e);
            return 1;
        }
    };

    // ── Basic validation ──────────────────────────────────────────────────────
    if config.teams.is_empty() {
        eprintln!("Error: config contains no teams.");
        return 1;
    }
    if config.weeks.is_empty() {
        eprintln!("Error: config contains no weeks.");
        return 1;
    }

    eprintln!(
        "Loaded config: {} teams, {} weeks",
        config.teams.len(),
        config.weeks.len()
    );
    eprintln!("Teams: {}", config.teams.join(", "));

    // ── Run the scheduler with a simple stderr progress bar ───────────────────
    eprintln!("Running scheduler…");

    // Track the last printed percentage so we only reprint when it changes.
    let last_pct = std::sync::Mutex::new(0u32);

    let result = run_scheduler_with_progress(&config, |progress| {
        let pct = (progress * 100.0) as u32;
        // Lock is cheap here — progress callbacks are infrequent.
        if let Ok(mut last) = last_pct.lock() {
            if pct != *last {
                eprint!("\r  {}%", pct);
                // Flush so the percentage updates in place on most terminals.
                use std::io::Write;
                let _ = std::io::stderr().flush();
                *last = pct;
            }
        }
    });

    // Move to the next line after the progress display.
    eprintln!();

    // ── Handle scheduler result ───────────────────────────────────────────────
    match result {
        Err(e) => {
            eprintln!("Scheduler error: {}", e);
            1
        }
        Ok(solutions) if solutions.is_empty() => {
            eprintln!(
                "No valid schedules found. \
                 Check that bye restrictions are not over-constrained."
            );
            1
        }
        Ok(solutions) => {
            print_results(&solutions, &config.teams, &config.weeks);
            0
        }
    }
}

// ── Config loading ────────────────────────────────────────────────────────────

/// Read a file and deserialise it as a `LeagueConfig`.
fn load_config(path: &str) -> Result<LeagueConfig, String> {
    if !Path::new(path).exists() {
        return Err(format!("file not found"));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice::<LeagueConfig>(&bytes).map_err(|e| {
        format!("JSON parse error: {}", e)
    })
}

// ── Output formatting ─────────────────────────────────────────────────────────

/// Print all ranked solutions to stdout.
///
/// The format is identical to the plain-text export produced by the GUI
/// (`Message::ExportResults`), so the same file can be saved or piped.
fn print_results(solutions: &[Solution], teams: &[String], weeks: &[u32]) {
    let mut sorted_weeks = weeks.to_vec();
    sorted_weeks.sort_unstable();

    println!();
    println!("Found {} solution(s).", solutions.len());

    for sol in solutions {
        println!();
        println!("==============================");
        println!("Rank:    {}", sol.rank);
        println!("Score:   {}", sol.score);
        println!("Penalty: {}", sol.penalty);

        // ── Bye-week table ────────────────────────────────────────────────────
        println!();
        println!("Bye Weeks");
        println!("---------");
        for team in teams {
            let week  = sol.bye_assignment.get(team).copied();
            let score = sol.bye_detail.get(team).copied().unwrap_or(0);

            match week {
                Some(w) => {
                    let score_label = match score {
                        -1 => " (excluded)".to_string(),
                        2  => " ✓ 1st choice".to_string(),
                        1  => " ✓ 2nd choice".to_string(),
                        0  => String::new(),
                        _  => String::new(),
                    };
                    println!("  {:20} week {:2}  [score {}{}]",
                        team, w, score, score_label);
                }
                None => {
                    // 6-team schedules have no byes.
                    println!("  {:20} no bye", team);
                }
            }
        }

        // ── Week-by-week match listing ────────────────────────────────────────
        println!();
        println!("Matches");
        println!("-------");
        for &w in &sorted_weeks {
            println!("  Week {}:", w);
            match sol.schedule.get(&w) {
                Some(games) if !games.is_empty() => {
                    for (host, away) in games {
                        println!("    {} hosts {}", host, away);
                    }
                }
                _ => {
                    println!("    (no games — bye week)");
                }
            }
        }
    }

    println!();
}

// ── Help text ─────────────────────────────────────────────────────────────────

fn print_usage(bin: &str) {
    println!(
        "Usage: {bin} [OPTIONS] [CONFIG]

Arguments:
  CONFIG    Path to a LeagueConfig JSON file produced by the GUI's Save feature.
            When omitted, the graphical wizard launches instead.

Options:
  -h, --help   Print this help message and exit.

Exit codes:
  0   Results printed successfully (or --help shown).
  1   File not found, parse error, or no valid schedules found.

Example:
  {bin} league.json
  {bin} league.json > schedules.txt",
        bin = bin
    );
}
