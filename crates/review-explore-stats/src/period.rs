use std::fmt;
use time::{
    Date, OffsetDateTime, PrimitiveDateTime, UtcOffset, format_description::well_known::Rfc3339,
    macros::format_description,
};

#[derive(Clone, Debug)]
pub struct Period {
    since: Option<Bound>,
    until: Option<Bound>,
}

#[derive(Clone, Debug)]
struct Bound {
    text: String,
    at_ms: i128,
}

#[derive(Clone, Copy)]
enum End {
    Since,
    Until,
}

impl Period {
    pub fn parse(since: Option<&str>, until: Option<&str>) -> eyre::Result<Self> {
        Self::parse_at(since, until, |local| {
            UtcOffset::local_offset_at(local.assume_utc()).unwrap_or(UtcOffset::UTC)
        })
    }

    pub(crate) fn parse_at(
        since: Option<&str>,
        until: Option<&str>,
        offset: impl Fn(PrimitiveDateTime) -> UtcOffset,
    ) -> eyre::Result<Self> {
        Ok(Self {
            since: since
                .map(|text| Bound::parse(text, End::Since, &offset))
                .transpose()?,
            until: until
                .map(|text| Bound::parse(text, End::Until, &offset))
                .transpose()?,
        })
    }

    pub(crate) fn contains(&self, at_ms: u64) -> bool {
        let at_ms = i128::from(at_ms);
        self.since.as_ref().is_none_or(|since| since.at_ms <= at_ms)
            && self.until.as_ref().is_none_or(|until| at_ms < until.at_ms)
    }
}

impl Bound {
    fn parse(
        text: &str,
        end: End,
        offset: impl Fn(PrimitiveDateTime) -> UtcOffset,
    ) -> eyre::Result<Self> {
        let at = if let Ok(at) = OffsetDateTime::parse(text, &Rfc3339) {
            at
        } else {
            let date =
                Date::parse(text, format_description!("[year]-[month]-[day]")).map_err(|_| {
                    eyre::eyre!(
                        "unknown time {text:?}: write a date such as 2026-09-15, \
                         or an RFC 3339 time such as 2026-09-15T14:00:00+02:00"
                    )
                })?;
            let day = match end {
                End::Since => date,
                End::Until => date
                    .next_day()
                    .ok_or_else(|| eyre::eyre!("{text:?} is too late"))?,
            };
            let midnight = day.midnight();
            midnight.assume_offset(offset(midnight))
        };
        Ok(Self {
            text: text.to_owned(),
            at_ms: at.unix_timestamp_nanos() / 1_000_000,
        })
    }
}

impl fmt::Display for Period {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (&self.since, &self.until) {
            (Some(since), Some(until)) => {
                write!(output, "from {} until {}", since.text, until.text)
            }
            (Some(since), None) => write!(output, "from {}", since.text),
            (None, Some(until)) => write!(output, "until {}", until.text),
            (None, None) => output.write_str("all time"),
        }
    }
}
