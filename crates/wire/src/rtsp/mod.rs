//! RTSP/1.0 messages as AirPlay 2 uses them: requests we send, and
//! responses or reverse requests (events channel) we read.

pub const USER_AGENT: &str = "AirPlay/670.6.2";
pub const PLIST: &str = "application/x-apple-binary-plist";

/// The headers every request carries.
pub struct Identity<'a> {
    pub dacp_id: &'a str,
    pub active_remote: &'a str,
}

pub fn request(
    method: &str,
    uri: &str,
    cseq: u32,
    identity: &Identity,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) -> Vec<u8> {
    let mut head = format!(
        "{method} {uri} RTSP/1.0\r\nCSeq: {cseq}\r\nUser-Agent: {USER_AGENT}\r\n\
         DACP-ID: {}\r\nActive-Remote: {}\r\n",
        identity.dacp_id, identity.active_remote
    );
    for (name, value) in extra_headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    let mut out = head.into_bytes();
    out.extend_from_slice(body);
    out
}

#[derive(Debug)]
pub struct Message {
    /// "RTSP/1.0 200 OK" for a response, "POST /command RTSP/1.0" for a request.
    pub start_line: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Message {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    /// The status code of a response; None for a request.
    pub fn status(&self) -> Option<u16> {
        let mut words = self.start_line.split_whitespace();
        if !words.next()?.starts_with("RTSP/") {
            return None;
        }
        words.next()?.parse().ok()
    }

    pub fn cseq(&self) -> Option<u32> {
        self.header("CSeq")?.trim().parse().ok()
    }

    /// The method of a request; None for a response.
    pub fn method(&self) -> Option<&str> {
        let first = self.start_line.split_whitespace().next()?;
        (!first.starts_with("RTSP/")).then_some(first)
    }
}

pub enum Parsed {
    /// A whole message and the number of bytes it took.
    Done(Message, usize),
    NeedMore,
    Invalid,
}

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 4 * 1024 * 1024;

/// Take one message from the front of `buffer`.
pub fn parse(buffer: &[u8]) -> Parsed {
    let Some(head_end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
        return if buffer.len() > MAX_HEAD { Parsed::Invalid } else { Parsed::NeedMore };
    };
    let Ok(head) = std::str::from_utf8(&buffer[..head_end]) else { return Parsed::Invalid };
    let mut lines = head.split("\r\n");
    let start_line = lines.next().unwrap_or_default().to_string();
    let headers: Vec<(String, String)> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(n, v)| (n.trim().to_string(), v.trim().to_string()))
        .collect();
    let length = headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("Content-Length"))
        .map_or(Some(0), |(_, v)| v.parse::<usize>().ok());
    let Some(length) = length.filter(|&l| l <= MAX_BODY) else { return Parsed::Invalid };
    let body_start = head_end + 4;
    if buffer.len() < body_start + length {
        return Parsed::NeedMore;
    }
    let body = buffer[body_start..body_start + length].to_vec();
    Parsed::Done(Message { start_line, headers, body }, body_start + length)
}

/// The reply to a reverse request on the events channel.
pub fn ok_reply(request: &Message) -> Vec<u8> {
    let mut out = String::from("RTSP/1.0 200 OK\r\nContent-Length: 0\r\nAudio-Latency: 0\r\n");
    if let Some(server) = request.header("Server") {
        out.push_str(&format!("Server: {server}\r\n"));
    }
    if let Some(cseq) = request.header("CSeq") {
        out.push_str(&format!("CSeq: {cseq}\r\n"));
    }
    out.push_str("\r\n");
    out.into_bytes()
}
