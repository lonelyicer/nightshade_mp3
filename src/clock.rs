pub const EMPTY_TIME: &str = "--:--";

pub const EMPTY_PROGRESS: &str = "--:--/--:--";

pub fn format_time(seconds: f64) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return EMPTY_TIME.to_owned();
    }

    let total = seconds.floor().clamp(0.0, 359_999.0) as u64;

    let hours = total / 3600;

    let minutes = (total % 3600) / 60;

    let seconds = total % 60;

    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn format_progress(position: f64, duration: f64) -> String {
    let position_text = if position.is_finite() && position >= 0.0 {
        format_time(position)
    } else {
        EMPTY_TIME.to_owned()
    };

    let duration_text = if duration.is_finite() && duration > 0.0 {
        format_time(duration)
    } else {
        EMPTY_TIME.to_owned()
    };

    format!("{position_text}/{duration_text}")
}
