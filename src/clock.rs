pub const EMPTY_TIME: &str = "--:--";

pub const EMPTY_PROGRESS: &str = "--:--/--:--";

const MAX_MINUTES: u64 = 999;

pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return EMPTY_TIME.to_owned();
    }

    let total_seconds = seconds.floor() as u64;

    let minutes = (total_seconds / 60).min(MAX_MINUTES);

    let seconds = total_seconds % 60;

    format!("{minutes:02}:{seconds:02}",)
}

pub fn format_progress(position: f64, duration: f64) -> String {
    let position = if position.is_finite() && position >= 0.0 {
        format_time(position)
    } else {
        EMPTY_TIME.to_owned()
    };

    let duration = if duration.is_finite() && duration > 0.0 {
        format_time(duration)
    } else {
        EMPTY_TIME.to_owned()
    };

    format!("{position}/{duration}")
}
