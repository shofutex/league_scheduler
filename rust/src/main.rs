//! Swim League Scheduler — iced 0.13
//! Wizard: Teams → Weeks → Base Schedule → Bye Preferences → Bye Restrictions → Results

mod config;
mod scheduler;
mod message;
mod update;
mod state;
mod view;

// Make sure we can get the items we need from the other modules
use crate::state::SwimScheduler;

use iced::Theme;

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



// ── App state ─────────────────────────────────────────────────────────────────







