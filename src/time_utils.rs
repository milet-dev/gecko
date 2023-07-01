use time::{format_description, OffsetDateTime, UtcOffset};

pub fn to_relative_time(seconds: i64) -> String {
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

pub fn to_datetime(time: OffsetDateTime, offset: Option<i32>) -> String {
    let format = format_description::parse(
        "[month repr:short] [day], [year], [hour repr:12]:[minute] [period] GMT[offset_hour padding:none sign:mandatory]",
    )
    .unwrap();
    let t = {
        match offset {
            Some(m) => {
                let utc_offset = UtcOffset::from_whole_seconds(m / 60).unwrap();
                time.to_offset(utc_offset)
            }
            None => time,
        }
    };
    t.format(&format).unwrap().to_string()
}
