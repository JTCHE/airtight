//! settings.json, next to the log. Same keys as the
//! Python app wrote, so an upgrade keeps the receiver and the switches.

use airtight_wire::json::{self, Json};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub receiver_name: Option<String>,
    pub receiver_host: Option<String>,
    /// The render endpoint to capture, by name; None follows the default.
    pub endpoint: Option<String>,
    pub volume: u8,
    pub latency_ms: u32,
    pub auto_connect: bool,
    pub auto_start: bool,
    /// The last receivers seen, (name, host), so the list shows at once.
    pub receivers: Vec<(String, String)>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            receiver_name: None,
            receiver_host: None,
            endpoint: None,
            volume: 35,
            latency_ms: 120,
            auto_connect: false,
            auto_start: false,
            receivers: Vec::new(),
        }
    }
}

fn path() -> PathBuf {
    airtight_log::directory().join("settings.json")
}

fn text(value: Option<&Json>) -> Option<String> {
    match value {
        Some(Json::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

fn number(value: Option<&Json>) -> Option<f64> {
    match value {
        Some(Json::Number(n)) => Some(*n),
        _ => None,
    }
}

impl Settings {
    pub fn parse(source: &str) -> Settings {
        let mut s = Settings::default();
        let Some(root) = json::parse(source) else { return s };
        s.receiver_name = text(root.get("receiver_name"));
        s.receiver_host = text(root.get("receiver_host"));
        s.endpoint = text(root.get("endpoint"));
        s.volume = number(root.get("volume")).map_or(s.volume, |v| v.clamp(0.0, 100.0) as u8);
        s.latency_ms = number(root.get("latency_ms")).map_or(s.latency_ms, |v| v.clamp(50.0, 2000.0) as u32);
        s.auto_connect = root.get("auto_connect") == Some(&Json::Bool(true));
        s.auto_start = root.get("auto_start") == Some(&Json::Bool(true));
        if let Some(Json::Array(list)) = root.get("receivers") {
            s.receivers = list
                .iter()
                .filter_map(|pair| match pair {
                    Json::Array(p) => Some((text(p.first())?, text(p.get(1))?)),
                    _ => None,
                })
                .collect();
        }
        s
    }

    pub fn to_json(&self) -> String {
        let text = |v: &Option<String>| v.as_ref().map_or(Json::Null, |s| Json::String(s.clone()));
        let receivers = self.receivers.iter().map(|(n, h)| Json::Array(vec![Json::String(n.clone()), Json::String(h.clone())]));
        json::write(&Json::Object(vec![
            ("receiver_name".into(), text(&self.receiver_name)),
            ("receiver_host".into(), text(&self.receiver_host)),
            ("endpoint".into(), text(&self.endpoint)),
            ("volume".into(), Json::Number(f64::from(self.volume))),
            ("latency_ms".into(), Json::Number(f64::from(self.latency_ms))),
            ("auto_connect".into(), Json::Bool(self.auto_connect)),
            ("auto_start".into(), Json::Bool(self.auto_start)),
            ("receivers".into(), Json::Array(receivers.collect())),
        ]))
    }

    pub fn load() -> Settings {
        std::fs::read_to_string(path()).map(|s| Settings::parse(&s)).unwrap_or_default()
    }

    /// Write through a temporary file, so a crash mid-write keeps the old file.
    pub fn save(&self) {
        let _ = std::fs::create_dir_all(airtight_log::directory());
        let temporary = path().with_extension("json.tmp");
        if std::fs::write(&temporary, self.to_json()).is_ok() {
            let _ = std::fs::rename(&temporary, path());
        }
    }
}
