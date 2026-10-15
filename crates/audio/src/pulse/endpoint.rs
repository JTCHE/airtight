//! The sinks, by the monitor source that records each one. The saved name
//! is a sink name (`pactl list short sinks`); none means the default sink.

use crate::Endpoint;

const DEFAULT_MONITOR: &str = "@DEFAULT_MONITOR@";

// ponytail: no sink list; listing needs libpulse's async API. Add it when
// a Linux UI needs a source menu.
pub fn list() -> Vec<Endpoint> {
    default().into_iter().collect()
}

pub fn default() -> Option<Endpoint> {
    Some(Endpoint { id: DEFAULT_MONITOR.into(), name: "Default output".into() })
}

pub fn find(name: Option<&str>) -> Option<Endpoint> {
    match name.filter(|n| !n.is_empty()) {
        Some(sink) => Some(Endpoint { id: format!("{}.monitor", sink.trim_end_matches(".monitor")), name: sink.into() }),
        None => default(),
    }
}
