//! JSON for the settings file: a reader for any document and a writer
//! that indents by two spaces.

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Option<Json> {
    let mut parser = Parser { bytes: text.as_bytes(), at: 0 };
    let value = parser.value(0)?;
    parser.space();
    (parser.at == parser.bytes.len()).then_some(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Parser<'_> {
    fn space(&mut self) {
        while self.bytes.get(self.at).is_some_and(u8::is_ascii_whitespace) {
            self.at += 1;
        }
    }

    fn eat(&mut self, word: &str) -> bool {
        let found = self.bytes[self.at..].starts_with(word.as_bytes());
        if found {
            self.at += word.len();
        }
        found
    }

    fn value(&mut self, depth: usize) -> Option<Json> {
        if depth > 32 {
            return None;
        }
        self.space();
        match *self.bytes.get(self.at)? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(Json::String),
            b't' if self.eat("true") => Some(Json::Bool(true)),
            b'f' if self.eat("false") => Some(Json::Bool(false)),
            b'n' if self.eat("null") => Some(Json::Null),
            _ => self.number(),
        }
    }

    fn number(&mut self) -> Option<Json> {
        let start = self.at;
        while self.bytes.get(self.at).is_some_and(|b| b"+-.eE0123456789".contains(b)) {
            self.at += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.at]).ok()?.parse().ok().map(Json::Number)
    }

    fn string(&mut self) -> Option<String> {
        self.at += 1;
        let mut out = String::new();
        loop {
            let c = *self.bytes.get(self.at)?;
            self.at += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => out.push(self.escape()?),
                _ => {
                    // copy one whole UTF-8 character
                    let start = self.at - 1;
                    while self.bytes.get(self.at).is_some_and(|b| b & 0xc0 == 0x80) {
                        self.at += 1;
                    }
                    out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).ok()?);
                }
            }
        }
    }

    fn escape(&mut self) -> Option<char> {
        let c = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(match c {
            b'n' => '\n',
            b't' => '\t',
            b'r' => '\r',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'u' => {
                let hex = std::str::from_utf8(self.bytes.get(self.at..self.at + 4)?).ok()?;
                self.at += 4;
                let unit = u32::from_str_radix(hex, 16).ok()?;
                // ponytail: no surrogate pairs; the settings never hold them
                char::from_u32(unit).unwrap_or('\u{fffd}')
            }
            other => other as char,
        })
    }

    fn array(&mut self, depth: usize) -> Option<Json> {
        self.at += 1;
        let mut items = Vec::new();
        self.space();
        if self.eat("]") {
            return Some(Json::Array(items));
        }
        loop {
            items.push(self.value(depth + 1)?);
            self.space();
            if self.eat("]") {
                return Some(Json::Array(items));
            }
            if !self.eat(",") {
                return None;
            }
        }
    }

    fn object(&mut self, depth: usize) -> Option<Json> {
        self.at += 1;
        let mut entries = Vec::new();
        self.space();
        if self.eat("}") {
            return Some(Json::Object(entries));
        }
        loop {
            self.space();
            if self.bytes.get(self.at) != Some(&b'"') {
                return None;
            }
            let key = self.string()?;
            self.space();
            if !self.eat(":") {
                return None;
            }
            entries.push((key, self.value(depth + 1)?));
            self.space();
            if self.eat("}") {
                return Some(Json::Object(entries));
            }
            if !self.eat(",") {
                return None;
            }
        }
    }
}

pub fn write(value: &Json) -> String {
    let mut out = String::new();
    write_value(&mut out, value, 0);
    out
}

fn write_value(out: &mut String, value: &Json, indent: usize) {
    let pad = |n: usize| "  ".repeat(n);
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => out.push_str(&format!("{}", *n as i64)),
        Json::Number(n) => out.push_str(&n.to_string()),
        Json::String(s) => write_string(out, s),
        Json::Array(items) if items.is_empty() => out.push_str("[]"),
        Json::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&pad(indent + 1));
                write_value(out, item, indent + 1);
            }
            out.push_str(&format!("\n{}]", pad(indent)));
        }
        Json::Object(entries) if entries.is_empty() => out.push_str("{}"),
        Json::Object(entries) => {
            out.push('{');
            for (i, (key, item)) in entries.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&pad(indent + 1));
                write_string(out, key);
                out.push_str(": ");
                write_value(out, item, indent + 1);
            }
            out.push_str(&format!("\n{}}}", pad(indent)));
        }
    }
}

fn write_string(out: &mut String, text: &str) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
