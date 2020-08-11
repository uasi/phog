use chrono::{TimeZone, Utc};
use egg_mode::RateLimit;

pub fn count(size: usize, word: &str) -> String {
    format!("{} {}{}", size, word, if size == 1 { "" } else { "s" })
}

pub fn print_rate_limit(rate_limit: &RateLimit) {
    let reset_datetime = Utc.timestamp(rate_limit.reset as i64, 0);
    log::info!(
        "rate limit; remaining={}, limit={}, reset={}|{}",
        rate_limit.remaining,
        rate_limit.limit,
        rate_limit.reset,
        reset_datetime.to_rfc3339()
    );
    if rate_limit.remaining <= 5 {
        println!(
            "info: Rate limit {}/{}, reset at {} .",
            rate_limit.remaining, rate_limit.limit, reset_datetime
        );
    }
}
