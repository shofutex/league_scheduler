//! # cli.rs
//!
//! Headless command-line interface for the Swim League Scheduler.
//!
//! When the binary is invoked with a path argument it skips the iced GUI
//! entirely, loads the `LeagueConfig` from the given JSON file, runs the
//! combinatorial scheduler, and prints the ranked results to stdout.
//!
//! ## Usage
//!
//! ```text
//! swim-scheduler [OPTIONS] <config.json>
//! swim-scheduler              # (no args) launch the GUI
//! ```
//!
//! ## Options
//!
//! | Flag          | Effect                                              |
//! |---------------|-----------------------------------------------------|
//! | *(none)*      | Print results in the human-readable text format     |
//! | `--csv`       | Print results as CSV (same format as "Export CSV")  |
//! | `-h`/`--help` | Print usage and exit 0                              |
//!
//! ## Exit codes
//!
//! | Code | Meaning                                              |
//! |------|------------------------------------------------------|
//! | 0    | Results printed successfully, or `--help` shown      |
//! | 1    | File not found, JSON parse error, bad arguments, or  |
//! |      | no valid schedules produced by the scheduler         |

use std::path::Path;

use crate::config::LeagueConfig;
use crate::scheduler::{run_scheduler_with_progress, format_results, format_results_csv};

// ── Output format ─────────────────────────────────────────────────────────────

/// Which output format to use when printing results.
enum OutputFormat {
    /// Human-readable text (default).
    Text,
    /// CSV matching the GUI's "Export CSV" output.
    Csv,
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Inspect `std::env::args` and decide whether to run in CLI mode.
///
/// Returns `Some(exit_code)` if the program should exit after this call
/// (CLI mode was detected and either succeeded or failed).
/// Returns `None` if no arguments were supplied and the GUI should start.
pub fn run_if_cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();

    // Collect flags and positional arguments separately so the order of
    // `--csv` relative to the filename doesn't matter.
    let mut flags: Vec<&str>     = vec![];
    let mut positional: Vec<&str> = vec![];

    for arg in args.iter().skip(1) {
        if arg.starts_with('-') {
            flags.push(arg.as_str());
        } else {
            positional.push(arg.as_str());
        }
    }

    // No arguments at all: launch the GUI.
    if flags.is_empty() && positional.is_empty() {
        return None;
    }

    // Help flag: print usage and exit cleanly regardless of other args.
    if flags.contains(&"--help") || flags.contains(&"-h") {
        print_usage(&args[0]);
        return Some(0);
    }

    // Determine the output format.
    let format = if flags.contains(&"--csv") {
        OutputFormat::Csv
    } else if flags.is_empty() {
        OutputFormat::Text
    } else {
        // Unknown flag.
        eprintln!("Unknown flag: {}", flags[0]);
        eprintln!("Run `{} --help` for usage.", args[0]);
        return Some(1);
    };

    // Exactly one positional argument expected: the config file path.
    match positional.as_slice() {
        [] => {
            eprintln!("Error: no config file specified.");
            eprintln!("Run `{} --help` for usage.", args[0]);
            Some(1)
        }
        [path] => Some(run_cli(path, format)),
        _ => {
            eprintln!("Error: too many arguments.");
            eprintln!("Run `{} --help` for usage.", args[0]);
            Some(1)
        }
    }
}

// ── CLI runner ────────────────────────────────────────────────────────────────

/// Load `path`, run the scheduler, and print results in `format`.
/// Returns 0 on success, 1 on any error.
fn run_cli(path: &str, format: OutputFormat) -> i32 {
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
            let out = match format {
                OutputFormat::Text => format_results(&solutions, &config.teams, &config.weeks),
                OutputFormat::Csv  => format_results_csv(&solutions, &config.teams, &config.weeks),
            };
            print!("{}", out);
            0
        }
    }
}

// ── Config loading ────────────────────────────────────────────────────────────

/// Read a file and deserialise it as a `LeagueConfig`.
fn load_config(path: &str) -> Result<LeagueConfig, String> {
    if !Path::new(path).exists() {
        return Err("file not found".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    serde_json::from_slice::<LeagueConfig>(&bytes)
        .map_err(|e| format!("JSON parse error: {}", e))
}

// ── Help text ─────────────────────────────────────────────────────────────────

fn print_usage(bin: &str) {
    println!(
        "Usage: {bin} [OPTIONS] [CONFIG]

Arguments:
  CONFIG    Path to a LeagueConfig JSON file produced by the GUI's Save feature.
            When omitted, the graphical wizard launches instead.

Options:
  --csv        Print results as CSV instead of human-readable text.
               Produces the same output as the GUI's \"Export CSV\" button.
  -h, --help   Print this help message and exit.

Exit codes:
  0   Results printed successfully (or --help shown).
  1   File not found, parse error, unknown flag, or no valid schedules found.

Examples:
  {bin} league.json               # human-readable text
  {bin} --csv league.json         # CSV to stdout
  {bin} --csv league.json > schedules.csv",
        bin = bin
    );
}
