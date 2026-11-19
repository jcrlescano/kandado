//! Secure links: the create dialog (D-32) and the list (D-33).
//!
//! Share on an item opens D-32 directly. The share item dialog (D-31) chooses
//! between people and a link; vaults are not shared (decision 14), so it
//! would ask nothing and is left out.

mod create_link;
mod links;

pub use create_link::{LinkDialog, LinkDialogEvent};
pub use links::{LinksEvent, LinksPage};

use time::{OffsetDateTime, UtcOffset};

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];

fn local(moment: OffsetDateTime) -> OffsetDateTime {
    moment.to_offset(UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC))
}

/// "18 NOV", as the D-33 table writes a day.
pub fn day(moment: OffsetDateTime) -> String {
    let moment = local(moment);
    format!(
        "{} {}",
        moment.day(),
        MONTHS[usize::from(u8::from(moment.month())) - 1]
    )
}

/// "19 NOV, 21:04", as D-32 writes when a new link expires.
pub fn day_and_time(moment: OffsetDateTime) -> String {
    let at = local(moment);
    format!("{}, {:02}:{:02}", day(moment), at.hour(), at.minute())
}

/// How long until a link expires: "IN 40 MIN", "IN 23 H", "IN 4 DAYS".
pub fn remaining(expires_at: OffsetDateTime, now: OffsetDateTime) -> String {
    let seconds = (expires_at - now).whole_seconds().max(0);
    match seconds {
        0..=3_599 => format!("IN {} MIN", (seconds / 60).max(1)),
        3_600..=86_399 => format!("IN {} H", seconds / 3_600),
        _ => {
            let days = seconds / 86_400;
            format!("IN {days} {}", if days == 1 { "DAY" } else { "DAYS" })
        }
    }
}

/// "0 OF 1", or "2 · NO LIMIT".
pub fn views(views: i32, max_views: Option<i32>) -> String {
    match max_views {
        Some(max) => format!("{views} OF {max}"),
        None => format!("{views} · NO LIMIT"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_time_reads_as_the_table_writes_it() {
        let now = OffsetDateTime::from_unix_timestamp(1_795_000_000).unwrap();
        assert_eq!(
            remaining(now + time::Duration::seconds(20), now),
            "IN 1 MIN"
        );
        assert_eq!(
            remaining(now + time::Duration::minutes(40), now),
            "IN 40 MIN"
        );
        assert_eq!(remaining(now + time::Duration::hours(23), now), "IN 23 H");
        assert_eq!(remaining(now + time::Duration::days(1), now), "IN 1 DAY");
        assert_eq!(remaining(now + time::Duration::days(24), now), "IN 24 DAYS");
        assert_eq!(remaining(now - time::Duration::days(1), now), "IN 1 MIN");
    }

    #[test]
    fn views_read_as_the_table_writes_them() {
        assert_eq!(views(0, Some(1)), "0 OF 1");
        assert_eq!(views(2, None), "2 · NO LIMIT");
    }
}
