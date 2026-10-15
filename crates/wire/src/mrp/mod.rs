//! MediaRemote over `POST /command`: the bodies that put our track on the
//! receiver's Now Playing screen, and the remote presses it sends back.
//! Shapes follow captures of an iPhone sender.

mod protobuf;

use crate::plist::Value;
use protobuf::Message;

const INFO: &str = "kMRMediaRemoteNowPlayingInfo";
const COMMAND_INFO: &str = "kMRMediaRemoteCommandInfo";

/// MRP playback states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackState {
    Playing = 1,
    Paused = 2,
    Stopped = 3,
}

pub struct Sender<'a> {
    pub dacp_id: &'a str,
    pub name: &'a str,
    pub session_uuid: &'a str,
    pub group_uuid: &'a str,
    /// Stable per sender: the receiver keys its sender list on it.
    pub device_uuid: &'a str,
    pub message_uuid: &'a str,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Seconds; zero when unknown.
    pub duration: f64,
    pub elapsed: f64,
    pub playing: bool,
    /// When `elapsed` was true, in seconds since 2001 (Apple's epoch).
    pub timestamp: f64,
    /// Stays the same for one track, so a late tag update edits the item in place.
    pub unique_id: i64,
    /// (identifier, JPEG bytes).
    pub artwork: Option<(String, Vec<u8>)>,
}

fn serialize(value: Value) -> Vec<u8> {
    crate::plist::serialize(&value)
}

fn command(kind: &str, params: Value) -> Vec<u8> {
    serialize(Value::dict(vec![("type", Value::string(kind)), ("params", params)]))
}

/// Register as a now-playing source. Must go first.
pub fn device_info(sender: &Sender) -> Vec<u8> {
    let mut inner = Message::default();
    inner.string(1, sender.device_uuid);
    inner.string(2, sender.name);
    inner.string(3, "iPhone");
    inner.string(4, "21F90");
    inner.string(5, "com.apple.Music");
    inner.varint(7, 1);
    inner.varint(8, 139);
    inner.varint(9, 1);
    inner.varint(10, 1);
    inner.string(12, "com.apple.Music");
    inner.varint(13, 1);
    inner.varint(14, 1);
    inner.varint(15, 1);
    inner.varint(17, 2);
    inner.string(19, sender.dacp_id);
    inner.varint(21, 1); // device class: iPhone
    inner.varint(22, 1);
    inner.string(31, "com.apple.podcasts");
    inner.string(39, "iPhone17,1");
    inner.string(41, sender.session_uuid);
    inner.string(42, sender.group_uuid);
    inner.string(43, "com.apple.iBooks");
    let mut envelope = Message::default();
    envelope.varint(1, 15); // DEVICE_INFO_MESSAGE
    envelope.bytes(20, &inner.into_bytes());
    envelope.string(85, sender.message_uuid);
    let wrapped = protobuf::length_prefixed(&envelope.into_bytes());
    serialize(Value::dict(vec![("params", Value::dict(vec![("data", Value::Data(wrapped))]))]))
}

fn info_key(name: &str) -> String {
    format!("{INFO}{name}")
}

fn timeline(track: &Track) -> Vec<(String, Value)> {
    vec![
        (info_key("ElapsedTime"), Value::Real(track.elapsed)),
        (info_key("PlaybackRate"), Value::Real(if track.playing { 1.0 } else { 0.0 })),
        (info_key("DefaultPlaybackRate"), Value::Real(1.0)),
        (info_key("Timestamp"), Value::Date(track.timestamp)),
    ]
}

fn npi(policy: &str, info: Vec<(String, Value)>) -> Vec<u8> {
    let params = Value::dict(vec![
        ("type", Value::string("npi-text")),
        ("mergePolicy", Value::string(policy)),
        ("params", Value::Dict(info)),
    ]);
    command("updateMRNowPlayingInfo", params)
}

/// The whole now-playing item. Replaces what the receiver shows, so the
/// artwork rides along every time.
pub fn now_playing(track: &Track) -> Vec<u8> {
    let mut info = vec![
        (info_key("Title"), Value::string(&track.title)),
        (info_key("Artist"), Value::string(&track.artist)),
        (info_key("Album"), Value::string(&track.album)),
    ];
    if track.duration > 0.0 {
        info.push((info_key("Duration"), Value::Real(track.duration)));
    }
    info.extend(timeline(track));
    info.push((info_key("MediaType"), Value::string("MRMediaRemoteMediaTypeMusic")));
    info.push((info_key("UniqueIdentifier"), Value::Int(track.unique_id)));
    if let Some((identifier, jpeg)) = &track.artwork {
        info.push((info_key("ArtworkIdentifier"), Value::string(identifier)));
        info.push((info_key("ArtworkMIMEType"), Value::string("image/jpeg")));
        info.push((info_key("ArtworkData"), Value::Data(jpeg.clone())));
    }
    npi("replace", info)
}

/// A timeline correction alone: seek, pause, resume.
pub fn progress(track: &Track) -> Vec<u8> {
    npi("update", timeline(track))
}

fn command_info(command: i64, enabled: bool, options: Option<Value>) -> Value {
    let mut entries = vec![
        ("kCommandInfoCommandKey", Value::Int(command)),
        ("kCommandInfoEnabledKey", Value::Bool(enabled)),
    ];
    if let Some(options) = options {
        entries.push(("kCommandInfoOptionsKey", options));
    }
    Value::Data(serialize(Value::dict(entries)))
}

fn options(entries: Vec<(&str, Value)>) -> Option<Value> {
    Some(Value::Dict(entries.into_iter().map(|(k, v)| (format!("{COMMAND_INFO}{k}"), v)).collect()))
}

pub fn supported_commands() -> Vec<u8> {
    let mut list = vec![
        command_info(26, true, options(vec![("ShuffleMode", Value::Int(1))])),
        command_info(25, true, options(vec![("RepeatMode", Value::Int(1))])),
        command_info(
            24,
            true,
            options(vec![
                ("CanBeControlledByScrubbingKey", Value::Bool(false)),
                ("SupportsReferencePosition", Value::Bool(false)),
            ]),
        ),
        command_info(18, false, options(vec![("PreferredIntervalsKey", Value::Array(vec![]))])),
        command_info(17, false, options(vec![("PreferredIntervalsKey", Value::Array(vec![]))])),
    ];
    list.extend([10, 11, 8, 9, 5, 4, 3, 2, 1, 0].map(|c| command_info(c, true, None)));
    let params = Value::dict(vec![("mrSupportedCommandsFromSender", Value::Array(list))]);
    command("updateMRSupportedCommands", params)
}

pub fn playback_state(state: PlaybackState) -> Vec<u8> {
    let params = Value::dict(vec![("mrPlaybackState", Value::Int(state as i64))]);
    command("updateMRPlaybackState", params)
}

pub fn now_playing_client(process_id: u32, name: &str) -> Vec<u8> {
    let mut client = Message::default();
    client.varint(1, u64::from(process_id));
    client.string(2, "com.apple.Music");
    client.string(7, name);
    let params = Value::dict(vec![("mrNowPlayingClient", Value::Data(client.into_bytes()))]);
    command("updateMRNowPlayingClient", params)
}

/// A press on the receiver's remote (Watch, HomePod top).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteCommand {
    Play,
    Pause,
    PlayPause,
    Next,
    Previous,
}

pub fn parse_remote_command(body: &[u8]) -> Option<RemoteCommand> {
    let root = crate::plist::parse(body)?;
    if root.get("type")?.as_str()? != "sendMediaRemoteCommand" {
        return None;
    }
    match root.get("value")?.as_str()? {
        "play" => Some(RemoteCommand::Play),
        "paus" => Some(RemoteCommand::Pause),
        "plps" => Some(RemoteCommand::PlayPause),
        "nitm" => Some(RemoteCommand::Next),
        "pitm" => Some(RemoteCommand::Previous),
        _ => None,
    }
}
