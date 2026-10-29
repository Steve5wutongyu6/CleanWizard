//! Minimal, strict-enough XML reader for Lemon rule packs.
//!
//! The original app parses rule packs with \`NSXMLParser\` (a SAX parser). We
//! keep the same tree shape: an element has a tag, attributes, text content
//! and ordered child elements. Handles XML declaration, processing
//! instructions, comments, CDATA, entity references and self-closing tags.

#[derive(Debug, Clone, PartialEq)]
pub struct Elem {
    pub tag: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Elem>,
    pub text: String,
}

impl Elem {
    pub fn attr(&self, key: &str) -> Option<&str> {
        let needle = key.to_ascii_lowercase();
        self.attrs
            .iter()
            .find(|(k, _)| k.to_ascii_lowercase() == needle)
            .map(|(_, v)| v.as_str())
    }

    pub fn child(&self, tag: &str) -> Option<&Elem> {
        self.children
            .iter()
            .find(|c| c.tag.eq_ignore_ascii_case(tag))
    }

    pub fn children_named<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Elem> + 'a {
        self.children
            .iter()
            .filter(move |c| c.tag.eq_ignore_ascii_case(tag))
    }
}

#[derive(Debug)]
pub struct XmlError {
    pub message: String,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "xml error: {}", self.message)
    }
}

impl std::error::Error for XmlError {}

impl XmlError {
    fn new(message: impl Into<String>) -> Self {
        XmlError {
            message: message.into(),
        }
    }
}

struct Reader<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Reader<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.pos..]
    }

    fn eof(&self) -> bool {
        self.pos >= self.s.len()
    }

    fn skip_ws(&mut self) {
        while let Some(c) = self.rest().chars().next() {
            if c.is_whitespace() {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.rest().starts_with(s)
    }

    fn skip_past(&mut self, marker: &str) -> Result<(), XmlError> {
        match self.rest().find(marker) {
            Some(idx) => {
                self.pos += idx + marker.len();
                Ok(())
            }
            None => Err(XmlError::new(format!("missing '{}'", marker))),
        }
    }

    fn skip_misc(&mut self) -> Result<(), XmlError> {
        loop {
            self.skip_ws();
            if self.starts_with("<?") {
                self.skip_past("?>")?;
            } else if self.starts_with("<!--") {
                self.skip_past("-->")?;
            } else if self.starts_with("<!DOCTYPE") || self.starts_with("<!doctype") {
                self.skip_past(">")?;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.pos;
        while let Some(c) = self.rest().chars().next() {
            if c.is_alphanumeric() || matches!(c, ':' | '_' | '-' | '.' | '+') {
                self.pos += c.len_utf8();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(XmlError::new(format!("expected name at byte {}", self.pos)));
        }
        Ok(self.s[start..self.pos].to_string())
    }

    fn element(&mut self) -> Result<Elem, XmlError> {
        if !self.starts_with("<") {
            return Err(XmlError::new(format!("expected '<' at byte {}", self.pos)));
        }
        self.pos += 1;
        let tag = self.name()?;
        let mut attrs: Vec<(String, String)> = Vec::new();
        loop {
            self.skip_ws();
            if self.starts_with("/>") {
                self.pos += 2;
                return Ok(Elem {
                    tag,
                    attrs,
                    children: Vec::new(),
                    text: String::new(),
                });
            }
            if self.starts_with(">") {
                self.pos += 1;
                break;
            }
            if self.eof() {
                return Err(XmlError::new(format!("unterminated tag <{}>", tag)));
            }
            let key = self.name()?;
            self.skip_ws();
            if self.starts_with("=") {
                self.pos += 1;
            } else {
                return Err(XmlError::new(format!("attribute '{}' lacks '='", key)));
            }
            self.skip_ws();
            let quote = self
                .rest()
                .chars()
                .next()
                .ok_or_else(|| XmlError::new("missing attribute value"))?;
            if quote != '"' && quote != '\'' {
                return Err(XmlError::new(format!(
                    "unquoted value for attribute '{}'",
                    key
                )));
            }
            self.pos += 1;
            let vstart = self.pos;
            match self.rest().find(quote) {
                Some(idx) => {
                    let value = decode_entities(&self.s[vstart..vstart + idx]);
                    self.pos += idx + 1;
                    attrs.push((key, value));
                }
                None => return Err(XmlError::new("unterminated attribute value")),
            }
        }

        let mut children = Vec::new();
        let mut text = String::new();
        loop {
            if self.eof() {
                return Err(XmlError::new(format!("unclosed element <{}>", tag)));
            }
            if self.starts_with("</") {
                self.pos += 2;
                let close = self.name()?;
                self.skip_ws();
                if !self.starts_with(">") {
                    return Err(XmlError::new("malformed closing tag"));
                }
                self.pos += 1;
                if !close.eq_ignore_ascii_case(&tag) {
                    return Err(XmlError::new(format!(
                        "mismatched </{}> for <{}>",
                        close, tag
                    )));
                }
                break;
            }
            if self.starts_with("<!--") {
                self.skip_past("-->")?;
                continue;
            }
            if self.starts_with("<![CDATA[") {
                self.pos += 9;
                match self.rest().find("]]>") {
                    Some(idx) => {
                        text.push_str(&self.s[self.pos..self.pos + idx]);
                        self.pos += idx + 3;
                    }
                    None => return Err(XmlError::new("unterminated CDATA")),
                }
                continue;
            }
            if self.starts_with("<?") {
                self.skip_past("?>")?;
                continue;
            }
            if self.starts_with("<") {
                let child = self.element()?;
                children.push(child);
                continue;
            }
            let next = self
                .rest()
                .find('<')
                .unwrap_or(self.rest().len());
            text.push_str(&decode_entities(&self.s[self.pos..self.pos + next]));
            self.pos += next;
        }
        Ok(Elem {
            tag,
            attrs,
            children,
            text,
        })
    }
}

pub fn parse(input: &str) -> Result<Elem, XmlError> {
    let input = input.trim_start_matches('\u{feff}');
    let mut r = Reader { s: input, pos: 0 };
    r.skip_misc()?;
    if r.eof() {
        return Err(XmlError::new("empty document"));
    }
    let root = r.element()?;
    r.skip_misc()?;
    if !r.eof() {
        return Err(XmlError::new(format!("trailing content at byte {}", r.pos)));
    }
    Ok(root)
}

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(idx) = rest.find('&') {
        out.push_str(&rest[..idx]);
        let after = &rest[idx..];
        match after.find(';').filter(|end| *end <= 10) {
            Some(end) => {
                let entity = &after[1..end];
                let decoded = match entity {
                    "amp" => Some("&".to_string()),
                    "lt" => Some("<".to_string()),
                    "gt" => Some(">".to_string()),
                    "quot" => Some("\"".to_string()),
                    "apos" => Some("'".to_string()),
                    other => {
                        if let Some(hex) = other.strip_prefix("#x").or(other.strip_prefix("#X")) {
                            u32::from_str_radix(hex, 16)
                                .ok()
                                .and_then(char::from_u32)
                                .map(|c| c.to_string())
                        } else if let Some(dec) = other.strip_prefix('#') {
                            dec.parse::<u32>()
                                .ok()
                                .and_then(char::from_u32)
                                .map(|c| c.to_string())
                        } else {
                            None
                        }
                    }
                };
                match decoded {
                    Some(d) => {
                        out.push_str(&d);
                        rest = &after[end + 1..];
                    }
                    None => {
                        out.push('&');
                        rest = &after[1..];
                    }
                }
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEMON: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN">
<garbage version="2" garbage_appstore="NO">
  <category title="系统垃圾" tips="Sys junk">
    <item title="日志" type="0" cleanType="sys" recommend="YES" bundleId="com.apple">
      <path type="0" value="~/Library/Logs/SPACE" level="1">
        <filters level="1">
          <filter column="filename" relation="contains" value="log"/>
          <filter column="filesize" relation="greater" value="1 MB"/>
        </filters>
      </path>
    </item>
  </category>
  <cautions>
    <caution cautionID="1" column="filename" bundleID="com.apple.Safari" appName="Safari" value="History.db"/>
  </cautions>
  <filters>
    <filter column="filepath" relation="contains" value="Protect"/>
  </filters>
</garbage>"#;

    #[test]
    fn parses_nested_rule_document() {
        let root = parse(LEMON).expect("root");
        assert_eq!(root.tag, "garbage");
        assert_eq!(root.attr("version"), Some("2"));
        let cat = root.child("category").unwrap();
        assert_eq!(cat.attr("title"), Some("系统垃圾"));
        let item = cat.child("item").unwrap();
        let path = item.child("path").unwrap();
        assert_eq!(path.attr("value"), Some("~/Library/Logs/SPACE"));
        let filters = path.child("filters").unwrap();
        let filters_children = filters.children_named("filter").count();
        assert_eq!(filters_children, 2);
        assert!(root.child("cautions").is_some());
        assert!(root.child("filters").is_some());
    }

    #[test]
    fn handles_cdata_comments_and_entities() {
        let doc = "<!-- c --><a x=\"1 &amp; 2\"><b><![CDATA[<raw>]]></b><!-- inner --><c>t &lt; </c></a>";
        let root = parse(doc).unwrap();
        assert_eq!(root.attr("x"), Some("1 & 2"));
        assert_eq!(root.child("b").unwrap().text, "<raw>");
        assert_eq!(root.child("c").unwrap().text.trim(), "t <");
    }

    #[test]
    fn self_closing_tags_work() {
        let root = parse("<root><leaf a=\"1\"/><leaf2/></root>").unwrap();
        assert_eq!(root.children.len(), 2);
        assert!(root.child("leaf").unwrap().children.is_empty());
    }

    #[test]
    fn rejects_mismatched_tags() {
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a>").is_err());
        assert!(parse("<a></a>junk").is_err());
    }

    #[test]
    fn numeric_entities_decode() {
        assert_eq!(decode_entities("&#65;&#x42;"), "AB");
        assert_eq!(decode_entities("a & b"), "a & b");
        assert_eq!(decode_entities("&unknown;"), "&unknown;");
    }
}
