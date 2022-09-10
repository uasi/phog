use indicatif::{ProgressBar, ProgressStyle};

use std::time::Duration;

pub fn new_spinner(msg: String) -> ProgressBar {
    let style = ProgressStyle::default_spinner()
        .tick_strings(&["", ".", "..", "...", "....", ".....", "... Done."])
        .template("{msg}{spinner}")
        .expect("Failed to create spinner");
    let spinner = ProgressBar::new(1).with_style(style);
    spinner.set_message(msg);
    spinner.enable_steady_tick(Duration::from_millis(160));
    spinner
}
