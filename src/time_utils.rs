use time::{format_description, OffsetDateTime, UtcOffset};

pub fn to_relative_time(input: i64) -> String {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let seconds = now - input;
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
        if days == 1 {
            return "1 day ago".to_owned();
        }
        let weeks = days / 7;
        if weeks >= 1 && weeks <= 3 {
            if weeks == 1 {
                return "1 week ago".to_owned();
            }
            return format!("{weeks} weeks ago");
        } else if weeks >= 4 {
            return "1 month ago".to_owned();
        }
        return format!("{days} days ago");
    }

    let months = days / 30;
    if months <= 11 {
        return format!("{months} months ago");
    }

    /* let years = months / 12;
    if years == 1 {
        return "1 year ago".to_owned();
    } else {
        return format!("{years} years ago");
    } */

    let time = OffsetDateTime::from_unix_timestamp(input).unwrap();
    to_datetime_format(time, None, "[month repr:short] [day], [year]")
}

pub fn to_datetime(time: OffsetDateTime, offset: Option<i32>) -> String {
    let format = format_description::parse(
        "[month repr:short] [day], [year], [hour repr:12]:[minute] [period] GMT[offset_hour padding:none sign:mandatory]",
    )
    .unwrap();
    let t = {
        match offset {
            Some(m) => {
                let utc_offset = UtcOffset::from_whole_seconds(m * 60).unwrap();
                time.to_offset(utc_offset)
            }
            None => time,
        }
    };
    t.format(&format).unwrap()
}

pub fn to_datetime_format(time: OffsetDateTime, offset: Option<i32>, format: &str) -> String {
    let format = format_description::parse(format).unwrap();
    let t = {
        match offset {
            Some(m) => {
                let utc_offset = UtcOffset::from_whole_seconds(m * 60).unwrap();
                time.to_offset(utc_offset)
            }
            None => time,
        }
    };
    t.format(&format).unwrap()
}
