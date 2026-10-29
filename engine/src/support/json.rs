//! Minimal, allocation-friendly JSON value + parser + serializer.
//!
//! Only the subset the engine needs: objects, arrays, strings, i64 numbers
//! (plus f64 fallback), booleans and null. Object key order is preserved on
//! serialization, which keeps the C ABI output stable and diffable.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    pub fn obj() -> Json {
        Json::Obj(BTreeMap::new())
    }
    pub fn arr() -> Json {
        Json::Arr(Vec::new())
    }

    pub fn insert<K: Into<String>, V: Into<Json>>(&mut self, key: K, value: V) -> &mut Self {
        if let Json::Obj(map) = self {
            map.insert(key.into(), value.into());
        }
        self
    }

    pub fn push<V: Into<Json>>(&mut self, value: V) -> &mut Self {
        if let Json::Arr(items) = self {
            items.push(value.into());
        }
        self
    }

    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            Json::Int(i) => Some(*i != 0),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Json::Int(i) => Some(*i),
            Json::Float(f) if f.fract() == 0.0 => Some(*f as i64),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Int(i) => Some(*i as f64),
            Json::Float(f) => Some(*f),
            Json::Str(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_arr(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(items) => Some(items.as_slice()),
            _ => None,
        }
    }

    /// Unwrap an object value, treating missing keys as null.
    pub fn field(&self, key: &str) -> &Json {
        self.get(key).unwrap_or(&Json::Null)
    }

    pub fn to_string_compact(&self) -> String {
        let mut out = String::with_capacity(64);
        write_compact(self, &mut out);
        out
    }
}

impl From<bool> for Json {
    fn from(v: bool) -> Self {
        Json::Bool(v)
    }
}
impl From<i64> for Json {
    fn from(v: i64) -> Self {
        Json::Int(v)
    }
}
impl From<i32> for Json {
    fn from(v: i32) -> Self {
        Json::Int(v as i64)
    }
}
impl From<u64> for Json {
    fn from(v: u64) -> Self {
        Json::Int(v as i64)
    }
}
impl From<f64> for Json {
    fn from(v: f64) -> Self {
        Json::Float(v)
    }
}
impl From<String> for Json {
    fn from(v: String) -> Self {
        Json::Str(v)
    }
}
impl From<&str> for Json {
    fn from(v: &str) -> Self {
        Json::Str(v.to_string())
    }
}
impl From<Vec<Json>> for Json {
    fn from(v: Vec<Json>) -> Self {
        Json::Arr(v)
    }
}

/// Anything that knows how to serialize itself into [`Json`].
pub trait ToValue {
    fn to_value(&self) -> Json;
}

/// Anything that can be rebuilt from [`Json`].
pub trait FromValue: Sized {
    fn from_value(value: &Json) -> Option<Self>;
}

impl ToValue for Json {
    fn to_value(&self) -> Json {
        self.clone()
    }
}
impl ToValue for String {
    fn to_value(&self) -> Json {
        Json::Str(self.clone())
    }
}
impl ToValue for &str {
    fn to_value(&self) -> Json {
        Json::Str(self.to_string())
    }
}
impl ToValue for i64 {
    fn to_value(&self) -> Json {
        Json::Int(*self)
    }
}
impl ToValue for u64 {
    fn to_value(&self) -> Json {
        Json::Int(*self as i64)
    }
}
impl ToValue for usize {
    fn to_value(&self) -> Json {
        Json::Int(*self as i64)
    }
}
impl ToValue for u32 {
    fn to_value(&self) -> Json {
        Json::Int(*self as i64)
    }
}
impl ToValue for bool {
    fn to_value(&self) -> Json {
        Json::Bool(*self)
    }
}
impl<T: ToValue> ToValue for Option<T> {
    fn to_value(&self) -> Json {
        match self {
            Some(v) => v.to_value(),
            None => Json::Null,
        }
    }
}
impl<T: ToValue> ToValue for Vec<T> {
    fn to_value(&self) -> Json {
        Json::Arr(self.iter().map(|x| x.to_value()).collect())
    }
}
impl<T: ToValue> ToValue for [T] {
    fn to_value(&self) -> Json {
        Json::Arr(self.iter().map(|x| x.to_value()).collect())
    }
}
impl<T: ToValue> From<&[T]> for Json {
    fn from(items: &[T]) -> Self {
        Json::Arr(items.iter().map(|x| x.to_value()).collect())
    }
}
impl From<u32> for Json {
    fn from(v: u32) -> Self {
        Json::Int(v as i64)
    }
}
impl From<u8> for Json {
    fn from(v: u8) -> Self {
        Json::Int(v as i64)
    }
}
impl From<usize> for Json {
    fn from(v: usize) -> Self {
        Json::Int(v as i64)
    }
}
impl Json {
    /// Serialize any [`ToValue`] implementor.
    pub fn of<T: ToValue + ?Sized>(value: &T) -> Json {
        value.to_value()
    }
}
impl ToValue for f64 {
    fn to_value(&self) -> Json {
        Json::Float(*self)
    }
}

impl FromValue for String {
    fn from_value(value: &Json) -> Option<Self> {
        value.as_str().map(|s| s.to_string())
    }
}
impl FromValue for Json {
    fn from_value(value: &Json) -> Option<Self> {
        Some(value.clone())
    }
}
impl FromValue for i64 {
    fn from_value(value: &Json) -> Option<Self> {
        value.as_i64()
    }
}
impl FromValue for u64 {
    fn from_value(value: &Json) -> Option<Self> {
        value.as_i64().and_then(|v| u64::try_from(v).ok())
    }
}
impl FromValue for usize {
    fn from_value(value: &Json) -> Option<Self> {
        value.as_i64().and_then(|v| usize::try_from(v).ok())
    }
}
impl FromValue for u32 {
    fn from_value(value: &Json) -> Option<Self> {
        let v = value.as_i64();
        match v {
            Some(n) if n >= 0 && n <= u32::MAX as i64 => Some(n as u32),
            _ => value.as_f64().map(|f| f as u32),
        }
    }
}
impl FromValue for bool {
    fn from_value(value: &Json) -> Option<Self> {
        value.as_bool()
    }
}
impl<T: FromValue> FromValue for Vec<T> {
    fn from_value(value: &Json) -> Option<Self> {
        value
            .as_arr()
            .map(|arr| arr.iter().filter_map(|x| T::from_value(x)).collect())
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(value: &Json) -> Option<Self> {
        match value {
            Json::Null => Some(None),
            v => T::from_value(v).map(Some),
        }
    }
}

impl From<Vec<Box<str>>> for Json {
    fn from(v: Vec<Box<str>>) -> Self {
        Json::Arr(v.into_iter().map(|s| Json::Str(s.into())).collect())
    }
}



fn write_compact(value: &Json, out: &mut String) {
    match value {
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Int(i) => out.push_str(&i.to_string()),
        Json::Float(f) => {
            if f.is_finite() {
                out.push_str(&format!("{}", f));
            } else {
                out.push_str("null");
            }
        }
        Json::Str(s) => write_string(s, out),
        Json::Arr(items) => {
            out.push('[');
            for (idx, item) in items.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_compact(item, out);
            }
            out.push(']');
        }
        Json::Obj(map) => {
            out.push('{');
            for (idx, (key, item)) in map.iter().enumerate() {
                if idx > 0 {
                    out.push(',');
                }
                write_string(key, out);
                out.push(':');
                write_compact(item, out);
            }
            out.push('}');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
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

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

pub fn parse(input: &str) -> Result<Json, String> {
    let mut p = Parser {
        bytes: input.as_bytes(),
        pos: 0,
    };
    p.skip_ws();
    let value = p.value()?;
    p.skip_ws();
    if p.pos != p.bytes.len() {
        return Err(format!("trailing input at byte {}", p.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.peek() {
            if c == b' ' || c == b'\n' || c == b'\r' || c == b'\t' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!(
                "expected '{}' at byte {}",
                c as char, self.pos
            ))
        }
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => Ok(Json::Str(self.string()?)),
            Some(b't') => {
                self.literal("true")?;
                Ok(Json::Bool(true))
            }
            Some(b'f') => {
                self.literal("false")?;
                Ok(Json::Bool(false))
            }
            Some(b'n') => {
                self.literal("null")?;
                Ok(Json::Null)
            }
            Some(c) if c == b'-' || c.is_ascii_digit() => self.number(),
            Some(_) => Err(format!("unexpected byte at {}", self.pos)),
            None => Err("unexpected end of input".to_string()),
        }
    }

    fn literal(&mut self, lit: &str) -> Result<(), String> {
        if self.bytes[self.pos..].starts_with(lit.as_bytes()) {
            self.pos += lit.len();
            Ok(())
        } else {
            Err(format!("expected literal '{}' at {}", lit, self.pos))
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect(b'{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(Json::Obj(map));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(b':')?;
            self.skip_ws();
            let value = self.value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
        Ok(Json::Obj(map))
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(format!("expected ',' or ']' at byte {}", self.pos)),
            }
        }
        Ok(Json::Arr(items))
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let c = self.peek().ok_or_else(|| "unterminated string".to_string())?;
            self.pos += 1;
            match c {
                b'"' => return Ok(out),
                b'\\' => {
                    let esc = self.peek().ok_or_else(|| "bad escape".to_string())?;
                    self.pos += 1;
                    match esc {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{0008}'),
                        b'f' => out.push('\u{000c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let code = self.hex4()?;
                            if (0xD800..0xDC00).contains(&code) {
                                // high surrogate: expect a following \uXXXX
                                if self.bytes.get(self.pos) == Some(&b'\\')
                                    && self.bytes.get(self.pos + 1) == Some(&b'u')
                                {
                                    self.pos += 2;
                                    let low = self.hex4()?;
                                    let combined = 0x10000
                                        + ((code - 0xD800) << 10)
                                        + (low.wrapping_sub(0xDC00) & 0x3FF);
                                    out.push(
                                        char::from_u32(combined).unwrap_or('\u{FFFD}'),
                                    );
                                } else {
                                    out.push('\u{FFFD}');
                                }
                            } else {
                                out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                            }
                        }
                        _ => return Err(format!("bad escape at byte {}", self.pos)),
                    }
                }
                b if b < 0x80 => out.push(b as char),
                b if b >= 0xC0 => {
                    // Multi-byte UTF-8: pass through intact.
                    self.pos -= 1;
                    let start = self.pos;
                    self.pos += 1;
                    while self
                        .bytes
                        .get(self.pos)
                        .map(|x| x & 0xC0 == 0x80)
                        .unwrap_or(false)
                    {
                        self.pos += 1;
                    }
                    let chunk = &self.bytes[start..self.pos];
                    let text = std::str::from_utf8(chunk)
                        .map_err(|e| format!("bad utf-8 at byte {}: {}", start, e))?;
                    out.push_str(text);
                }
                b => return Err(format!("invalid byte 0x{:02x} in string", b)),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.bytes.len() {
            return Err("truncated unicode escape".to_string());
        }
        let slice = &self.bytes[self.pos..self.pos + 4];
        let mut val = 0u32;
        for b in slice {
            val = val
                .checked_mul(16)
                .ok_or_else(|| "unicode escape overflow".to_string())?;
            val += match b {
                b'0'..=b'9' => (*b - b'0') as u32,
                b'a'..=b'f' => (*b - b'a' + 10) as u32,
                b'A'..=b'F' => (*b - b'A' + 10) as u32,
                _ => return Err("bad unicode escape".to_string()),
            };
        }
        self.pos += 4;
        Ok(val)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let mut is_float = false;
        while let Some(c) = self.peek() {
            match c {
                b'0'..=b'9' => self.pos += 1,
                b'.' | b'e' | b'E' | b'+' | b'-' => {
                    is_float = true;
                    self.pos += 1;
                }
                _ => break,
            }
        }
        let text = &input_slice(self.bytes, start, self.pos)?;
        if is_float {
            text.parse::<f64>()
                .map(Json::Float)
                .map_err(|e| format!("bad number '{}': {}", text, e))
        } else {
            text.parse::<i64>()
                .map(Json::Int)
                .map_err(|e| format!("bad number '{}': {}", text, e))
        }
    }
}

fn input_slice(bytes: &[u8], start: usize, end: usize) -> Result<&str, String> {
    std::str::from_utf8(&bytes[start..end]).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj_of(json: &str) -> Json {
        parse(json).expect("parse ok")
    }

    #[test]
    fn parses_plain_values() {
        assert_eq!(obj_of("null"), Json::Null);
        assert_eq!(obj_of("true"), Json::Bool(true));
        assert_eq!(obj_of("false"), Json::Bool(false));
        assert_eq!(obj_of("-42"), Json::Int(-42));
        assert_eq!(obj_of("3.5"), Json::Float(3.5));
        assert_eq!(obj_of("\"hi\""), Json::Str("hi".into()));
    }

    #[test]
    fn parses_nested_objects_and_arrays() {
        let v = obj_of(r#"{"a":[1,2,{"b":"c"}],"d":{"e":null}}"#);
        assert_eq!(v.get("a").unwrap().as_arr().unwrap().len(), 3);
        assert_eq!(
            v.get("d").unwrap().get("e").unwrap(),
            &Json::Null
        );
    }

    #[test]
    fn escapes_roundtrip() {
        let s = "a\"b\\c\nd\te\u{00e9}";
        let json = Json::from(s).to_string_compact();
        let back = parse(&json).unwrap();
        assert_eq!(back.as_str().unwrap(), s);
    }

    #[test]
    fn surrogate_pairs_decode() {
        let v = parse(r#""\ud83d\ude00""#).unwrap();
        assert_eq!(v.as_str().unwrap(), "\u{1F600}");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("{").is_err());
        assert!(parse("[1,]").is_err());
        assert!(parse("{} trailing").is_err());
    }

    #[test]
    fn empty_containers() {
        assert_eq!(parse("{}").unwrap(), Json::obj());
        assert_eq!(parse("[]").unwrap(), Json::arr());
    }

    #[test]
    fn compact_string_is_parseable() {
        let mut o = Json::obj();
        o.insert("path", "/tmp/a b")
            .insert("size", 1024i64)
            .insert("ok", true);
        let mut arr = Json::arr();
        arr.push(Json::obj());
        o.insert("list", arr);
        let s = o.to_string_compact();
        let back = parse(&s).unwrap();
        assert_eq!(back.get("path").unwrap().as_str().unwrap(), "/tmp/a b");
        assert_eq!(back.get("size").unwrap().as_i64().unwrap(), 1024);
    }
}
