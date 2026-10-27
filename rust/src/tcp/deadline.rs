use serde_json::{json, Value};
use std::{
    io,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
#[derive(Clone, Copy)]
pub(super) struct Deadline {
    instant: Instant,
    unix_ms: u64,
}
impl Deadline {
    pub fn new(instant: Instant) -> Self {
        let remaining = instant.saturating_duration_since(Instant::now());
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .saturating_add(remaining)
            .as_millis()
            .min(u64::MAX as u128) as u64;
        Self { instant, unix_ms }
    }
    pub fn apply(self, value: &mut Value) -> io::Result<()> {
        if self
            .instant
            .checked_duration_since(Instant::now())
            .is_none_or(|duration| duration < Duration::from_millis(1))
        {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "TCP deadline exceeded",
            ));
        }
        value["deadlineMs"] = json!(self.unix_ms);
        Ok(())
    }
}
