use indicatif::{ProgressBar, ProgressStyle};

pub fn new_spinner(msg: &str) -> ProgressBar {
    let style = ProgressStyle::default_spinner()
        .tick_strings(&["", ".", "..", "...", "....", ".....", "... Done."])
        .template("{msg}{spinner}");
    let spinner = ProgressBar::new(1).with_style(style);
    spinner.set_message(msg);
    spinner.enable_steady_tick(160);
    spinner
}
