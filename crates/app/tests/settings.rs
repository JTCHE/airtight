use airtight_app::settings::Settings;

#[test]
fn reads_the_python_file_and_writes_it_back() {
    let python = r#"{
  "receiver_name": "Living Room",
  "receiver_host": "192.168.1.20",
  "endpoint": "Speakers",
  "volume": 85,
  "latency_ms": 250,
  "distro": "Ubuntu",
  "auto_connect": true,
  "auto_start": true,
  "receivers": [["Living Room", "192.168.1.20"]]
}"#;
    let s = Settings::parse(python);
    assert_eq!(s.receiver_host.as_deref(), Some("192.168.1.20"));
    assert_eq!((s.volume, s.latency_ms, s.auto_connect, s.auto_start), (85, 250, true, true));
    assert_eq!(s.receivers, vec![("Living Room".into(), "192.168.1.20".into())]);
    assert_eq!(Settings::parse(&s.to_json()), s);
    assert_eq!(Settings::parse("not json"), Settings::default());
}
