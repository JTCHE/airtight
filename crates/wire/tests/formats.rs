//! Wire formats against fixtures from Python's plistlib and against
//! round trips.

use airtight_wire::{alac, dns, json, mrp, plist, plist::Value, rtp, rtsp, tlv8};

fn fixture(name: &str) -> Vec<u8> {
    let line = include_str!("fixtures.txt").lines().find(|l| l.starts_with(name)).unwrap();
    let text = line.split_whitespace().nth(1).unwrap();
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}

#[test]
fn plist_reads_python_output() {
    let root = plist::parse(&fixture("plist")).unwrap();
    let stream = &root.get("streams").unwrap().as_array().unwrap()[0];
    assert_eq!(stream.get("dataPort").unwrap().as_int(), Some(51234));
    assert_eq!(stream.get("type").unwrap().as_int(), Some(96));
    assert_eq!(root.get("eventPort").unwrap().as_int(), Some(49300));
    assert_eq!(root.get("name").unwrap().as_str(), Some("Café été"));
    assert_eq!(root.get("neg").unwrap().as_int(), Some(-5));
    assert_eq!(root.get("big").unwrap().as_int(), Some(1 << 40));
    assert_eq!(root.get("r"), Some(&Value::Real(1.5)));
    assert_eq!(root.get("yes"), Some(&Value::Bool(true)));
    assert_eq!(root.get("blob"), Some(&Value::Data((0..20).collect())));
    assert_eq!(root.get("date"), Some(&Value::Date(789015845.0)));
    assert_eq!(root.get("long").unwrap().as_str().unwrap().len(), 300);
    assert_eq!(root.get("many").unwrap().as_array().unwrap()[299].as_int(), Some(299));
}

#[test]
fn plist_round_trips() {
    let many: Vec<Value> = (0..300).map(Value::Int).collect();
    let root = Value::dict(vec![
        ("a", Value::string("Café")),
        ("b", Value::Int(-7)),
        ("c", Value::Array(many)),
        ("d", Value::Data(vec![1; 70_000])),
        ("e", Value::Date(1.25)),
        ("f", Value::dict(vec![("g", Value::Bool(false))])),
    ]);
    assert_eq!(plist::parse(&plist::serialize(&root)), Some(root));
    assert_eq!(plist::parse(b"bplist00"), None);
}

#[test]
fn tlv8_fragments_long_values() {
    let long = vec![7u8; 300];
    let bytes = tlv8::encode(&[(tlv8::STATE, &[1]), (tlv8::PUBLIC_KEY, &long), (tlv8::SALT, &[9])]);
    assert_eq!(bytes.len(), 3 + 2 + 255 + 2 + 45 + 3);
    let items = tlv8::decode(&bytes).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(tlv8::find(&items, tlv8::PUBLIC_KEY), Some(&long[..]));
    assert!(tlv8::decode(&[6, 5, 1]).is_none());
}

#[test]
fn rtsp_frames_requests_and_parses_replies() {
    let identity = rtsp::Identity { dacp_id: "AB", active_remote: "12" };
    let request = rtsp::request("GET", "/info", 3, &identity, &[], b"");
    let text = String::from_utf8(request).unwrap();
    assert!(text.starts_with("GET /info RTSP/1.0\r\nCSeq: 3\r\n"));
    assert!(text.ends_with("Content-Length: 0\r\n\r\n"));

    let reply = b"RTSP/1.0 200 OK\r\nCSeq: 3\r\nContent-Length: 2\r\n\r\nhiRTSP";
    let rtsp::Parsed::Done(message, used) = rtsp::parse(reply) else { panic!() };
    assert_eq!((message.status(), message.cseq(), used), (Some(200), Some(3), reply.len() - 4));
    assert_eq!(message.body, b"hi");
    assert!(matches!(rtsp::parse(b"RTSP/1.0 200 OK\r\nContent-Length: 5\r\n\r\nab"), rtsp::Parsed::NeedMore));
}

#[test]
fn mrp_remote_command_round_trip() {
    let body = plist::serialize(&Value::dict(vec![
        ("type", Value::string("sendMediaRemoteCommand")),
        ("value", Value::string("plps")),
    ]));
    assert_eq!(mrp::parse_remote_command(&body), Some(mrp::RemoteCommand::PlayPause));
    let state = plist::parse(&mrp::playback_state(mrp::PlaybackState::Paused)).unwrap();
    assert_eq!(state.get("params").unwrap().get("mrPlaybackState").unwrap().as_int(), Some(2));
    let commands = plist::parse(&mrp::supported_commands()).unwrap();
    let list = commands.get("params").unwrap().get("mrSupportedCommandsFromSender").unwrap();
    assert_eq!(list.as_array().unwrap().len(), 15);
}

#[test]
fn alac_escape_frame_layout() {
    let samples: Vec<i16> = (0..704).map(|i| i as i16 - 352).collect();
    let frame = alac::encode(&samples);
    // 23 header bits + 704 * 16 sample bits + 3 end bits
    assert_eq!(frame.len(), (23 + 704 * 16 + 3usize).div_ceil(8));
    assert_eq!(frame[0], 0x20);
    assert_eq!(frame[2] & 0x02, 0x02); // escape bit
    // the first sample, -352 = 0xfea0, starts at bit 23
    assert_eq!(frame[2] & 1, 1);
    assert_eq!(frame[3], 0xfd);
}

#[test]
fn rtp_packets_open_with_the_key() {
    let key = [3u8; 32];
    let packet = rtp::audio_packet(&key, true, 0x1234, 99, b"hello");
    assert_eq!(&packet[..4], &[0x80, 0xe0, 0x12, 0x34]);
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&packet[packet.len() - 8..]);
    let mut payload = packet[12..17].to_vec();
    let tag = &packet[17..33];
    assert!(airtight_crypto::chacha::open(&key, &nonce, &packet[4..12], &mut payload, tag));
    assert_eq!(payload, b"hello");
    let sync = rtp::sync_packet(true, 1000, 5, 7);
    assert_eq!(u32::from_be_bytes(sync[4..8].try_into().unwrap()), 1000 + 11035);
}

#[test]
fn dns_finds_a_receiver() {
    let query = dns::query(dns::AIRPLAY);
    assert_eq!(&query[4..6], &[0, 1]);
    let records = vec![
        dns::Record::Ptr { name: dns::AIRPLAY.into(), target: format!("Kitchen.{}", dns::AIRPLAY) },
        dns::Record::Srv { name: format!("Kitchen.{}", dns::AIRPLAY), target: "hp.local".into(), port: 7000 },
        dns::Record::A { name: "hp.local".into(), address: [10, 0, 0, 5].into() },
    ];
    let found = dns::services(&records, dns::AIRPLAY);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].name.as_str(), found[0].port), ("Kitchen", 7000));
}

#[test]
fn json_reads_and_writes_settings() {
    let text = r#"{"receiver_name": "Living Room", "volume": 85, "auto": true,
                  "receivers": [["A", "10.0.0.1"]], "endpoint": null, "s": "é\n"}"#;
    let value = json::parse(text).unwrap();
    assert_eq!(value.get("volume"), Some(&json::Json::Number(85.0)));
    assert_eq!(value.get("s"), Some(&json::Json::String("é\n".into())));
    assert_eq!(json::parse(&json::write(&value)), Some(value));
    assert!(json::parse("{\"a\": }").is_none());
}
