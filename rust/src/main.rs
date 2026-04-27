//! # main.rs
//!
//! Application entry point for the Swim League Scheduler.
//!
//! ## Crate layout
//!
//! | Module       | Responsibility                                              |
//! |------------- |-------------------------------------------------------------|
//! | `config`     | `LeagueConfig` struct, serialisation, built-in schedules    |
//! | `message`    | `Message` enum — every event the update loop handles        |
//! | `state`      | `SwimScheduler` struct + `Step` enum                        |
//! | `update`     | `SwimScheduler::update()` — all state mutations             |
//! | `view`       | `SwimScheduler::view()` — pure widget-tree construction     |
//! | `scheduler`  | Combinatorial scheduling engine (rayon-parallel)            |
//!
//! ## Framework
//!
//! Built on [iced](https://github.com/iced-rs/iced) 0.13, which follows the
//! Elm Architecture:
//! - `view(&state)` → widget tree
//! - User interaction / async result → `Message`
//! - `update(&mut state, msg)` → new state (+ optional `Task`)
//!
//! The application is single-window, 900 × 700 px, using the built-in
//! `TokyoNight` theme.  The Inter font is bundled at compile time via
//! `include_bytes!` so no system font installation is required.

mod config;
mod scheduler;
mod message;
mod update;
mod state;
mod view;

use crate::state::SwimScheduler;
use iced::Theme;

/// Inter Regular, bundled at compile time.
///
/// `include_bytes!` embeds the font binary directly into the executable, so
/// the app renders consistently regardless of the fonts installed on the host
/// system.  The path is relative to `src/` (Cargo's convention for
/// `include_bytes!` in `main.rs`).
static INTER: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");

fn main() -> iced::Result {
    iced::application(
        "Swim League Scheduler",  // window title
        SwimScheduler::update,    // update function (Elm-style reducer)
        SwimScheduler::view,      // view function (pure render)
    )
    // Use iced's built-in Tokyo Night dark theme for the default widget styles.
    .theme(|_| Theme::TokyoNight)
    // Fixed initial window size; the user can resize freely after launch.
    .window_size((900.0, 700.0))
    // Register the bundled Inter font bytes with iced's font system.
    .font(INTER)
    // Set Inter as the default font so all `text()` widgets use it
    // without needing per-widget font specifications.
    .default_font(iced::Font::with_name("Inter"))
    // `run_with` calls `SwimScheduler::new()` to produce the initial state
    // and any startup `Task` (none in this app).
    .run_with(SwimScheduler::new)
}
