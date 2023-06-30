pub fn to_string(seconds: i64) -> String {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let seconds = now - seconds;
    if seconds <= 10 {
        return "now".to_string();
    } else if seconds <= 59 {
        return format!("{seconds} seconds ago");
    }
    let minutes = seconds / 60;
    if minutes <= 59 {
        return format!("{minutes} minutes ago");
    }
    let hours = minutes / 60;
    if hours <= 23 {
        return format!("{hours} hours ago");
    }
    let days = hours / 24;
    if days <= 30 {
        return format!("{days} days ago");
    }
    let months = days / 30;
    if months <= 12 {
        return format!("{months} months ago");
    }
    format!("{seconds}")
}
