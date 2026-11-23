// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's XML files: the `#include` line preprocessing and the lenient,
//! non-validating syntax of docs/formats/ui-layout.md (sections 1.2–1.4),
//! which docs/formats/text-and-fonts.md (section 1.3) shares for string
//! tables.
//!
//! Text is Windows-1251 whatever the declaration says; names, attribute values
//! and text are decoded with [`crate::cp1251`] (a bijection, so every byte
//! survives as one character).

use std::io;

use crate::cp1251;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// XML white space as the specs use it: tab, LF, VT, FF, CR and space
/// ("space, tab, CR, LF, other ASCII whitespace"). Not U+00A0: byte `0xA0`
/// is a no-break space glyph of the text.
pub fn is_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ')
}

fn is_space_byte(b: u8) -> bool {
    matches!(b, 0x09..=0x0D | 0x20)
}

/// `s` without leading and trailing [`is_space`] characters.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// Deepest include nesting accepted. The specs do not cover recursive
/// includes; the shipped data nests at most twice. A deeper chain is taken
/// for a cycle and rejected.
pub const MAX_INCLUDE_DEPTH: usize = 32;

/// Expands `#include` lines (ui-layout.md section 1.2, text-and-fonts.md
/// section 1.3): the text is split into lines at CR/LF runs (so empty lines
/// disappear); a line whose first byte is `#` and which contains `#include`
/// is replaced by the preprocessed contents of the file named between its
/// first pair of double quotes, which `include` reads (it gets the path as
/// written). Other lines are kept. Lines are joined with LF.
pub fn preprocess(
    text: &[u8],
    include: &mut dyn FnMut(&[u8]) -> io::Result<Vec<u8>>,
) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    preprocess_into(text, include, 0, &mut out)?;
    Ok(out)
}

fn preprocess_into(
    text: &[u8],
    include: &mut dyn FnMut(&[u8]) -> io::Result<Vec<u8>>,
    depth: usize,
    out: &mut Vec<u8>,
) -> io::Result<()> {
    for line in text
        .split(|&b| b == b'\r' || b == b'\n')
        .filter(|l| !l.is_empty())
    {
        if line.first() == Some(&b'#') && line.windows(8).any(|w| w == b"#include") {
            let path = include_path(line).ok_or_else(|| {
                invalid(format!(
                    "include line without a quoted path: {}",
                    cp1251::decode(line)
                ))
            })?;
            if depth >= MAX_INCLUDE_DEPTH {
                return Err(invalid(format!(
                    "includes nested deeper than {MAX_INCLUDE_DEPTH} at {}",
                    cp1251::decode(path)
                )));
            }
            let included = include(path)?;
            preprocess_into(&included, include, depth + 1, out)?;
        } else {
            out.extend_from_slice(line);
            out.push(b'\n');
        }
    }
    Ok(())
}

/// The bytes between the first pair of `"`. The specs name no other form;
/// a line without a closing quote is rejected.
fn include_path(line: &[u8]) -> Option<&[u8]> {
    let open = line.iter().position(|&b| b == b'"')?;
    let rest = &line[open + 1..];
    let close = rest.iter().position(|&b| b == b'"')?;
    Some(&rest[..close])
}

/// A node of an element's content.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    Element(Element),
    /// Character data (entities decoded, CDATA included), never only white
    /// space; adjacent pieces (around comments, CDATA) are merged.
    Text(String),
}

/// An element: its name, attributes in document order and content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<(String, String)>,
    pub children: Vec<Node>,
}

impl Element {
    /// The value of the first attribute named exactly `name`.
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// The child elements in document order.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The child elements named exactly `name`, in document order.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> {
        self.elements().filter(move |e| e.name == name)
    }

    /// The first child element named exactly `name`.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// The raw text of the first child when it is text (ui-layout.md
    /// section 1.4); `None` when the element is empty or starts with an
    /// element.
    pub fn raw_value(&self) -> Option<&str> {
        match self.children.first() {
            Some(Node::Text(t)) => Some(t),
            _ => None,
        }
    }

    /// [`Self::raw_value`] without leading and trailing white space
    /// (ui-layout.md section 1.3).
    pub fn value(&self) -> Option<&str> {
        self.raw_value().map(trim)
    }

    /// The element at `path` (`:`-separated names) below this one: each
    /// component takes the first child of that name.
    pub fn find(&self, path: &str) -> Option<&Element> {
        path.split(':').try_fold(self, |e, name| e.child(name))
    }
}

/// A parsed document: the top-level nodes (included fragments may add
/// several top-level elements).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub nodes: Vec<Node>,
}

impl Document {
    /// The first element of the document (ui-layout.md section 1.4).
    pub fn root(&self) -> Option<&Element> {
        self.nodes.iter().find_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The element at `path` relative to the root, the first component
    /// taking the `index`-th (0-based) root child of that name
    /// (ui-layout.md section 1.4).
    pub fn find(&self, path: &str, index: usize) -> Option<&Element> {
        let root = self.root()?;
        let (first, rest) = match path.split_once(':') {
            Some((first, rest)) => (first, Some(rest)),
            None => (path, None),
        };
        let element = root.elements().filter(|e| e.name == first).nth(index)?;
        match rest {
            Some(rest) => element.find(rest),
            None => Some(element),
        }
    }

    /// The number of root children named `name`.
    pub fn count(&self, name: &str) -> usize {
        self.root()
            .map_or(0, |root| root.children_named(name).count())
    }
}

/// Parses `text` (already preprocessed) leniently (ui-layout.md section 1.3):
/// attribute values in double or single quotes or unquoted (ending at white
/// space, `/` or `>`), attributes need not be separated by white space, a
/// comment ends at the first `-->`, processing instructions (the `<?xml …?>`
/// declaration anywhere, as in included files) and `<!…>` declarations are
/// skipped, the five predefined entities and numeric character references are
/// decoded, CDATA sections are text. Text outside any element is dropped.
///
/// Choices where the spec is silent: an end tag closes the innermost open
/// element whatever its name (the shipped `ingame_msglog_*.xml` open
/// `<_window>` and close `</window>`); an end tag with no open element, an
/// element still open at the end, an unterminated markup construct or a
/// numeric reference to a character Windows-1251 lacks is an error; an
/// unknown entity is kept literally; an attribute without `=` has an empty
/// value.
pub fn parse(text: &[u8]) -> io::Result<Document> {
    Parser { text, pos: 0 }.document()
}

struct Parser<'a> {
    text: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> io::Error {
        let line = 1 + self.text[..self.pos.min(self.text.len())]
            .iter()
            .filter(|&&b| b == b'\n')
            .count();
        invalid(format!("XML line {line}: {message}"))
    }

    fn rest(&self) -> &[u8] {
        &self.text[self.pos..]
    }

    fn starts_with(&self, s: &[u8]) -> bool {
        self.rest().starts_with(s)
    }

    /// Advances past the next `end`, or fails.
    fn skip_past(&mut self, end: &[u8], what: &str) -> io::Result<&[u8]> {
        let start = self.pos;
        match self.rest().windows(end.len()).position(|w| w == end) {
            Some(i) => {
                self.pos += i + end.len();
                Ok(&self.text[start..start + i])
            }
            None => Err(self.error(&format!("unterminated {what}"))),
        }
    }

    fn skip_space(&mut self) {
        while self.pos < self.text.len() && is_space_byte(self.text[self.pos]) {
            self.pos += 1;
        }
    }

    fn document(mut self) -> io::Result<Document> {
        // The stack of open elements; the bottom is a pseudo-element holding
        // the top-level nodes.
        let mut stack = vec![Element::default()];
        let mut text = Vec::new();
        while self.pos < self.text.len() {
            if self.text[self.pos] != b'<' {
                let end = self
                    .rest()
                    .iter()
                    .position(|&b| b == b'<')
                    .map_or(self.text.len(), |i| self.pos + i);
                text.extend_from_slice(&self.text[self.pos..end]);
                self.pos = end;
            } else if self.starts_with(b"<!--") {
                self.pos += 4;
                self.skip_past(b"-->", "comment")?;
            } else if self.starts_with(b"<![CDATA[") {
                self.flush_text(&mut stack, &mut text)?;
                self.pos += 9;
                let data = cp1251::decode(self.skip_past(b"]]>", "CDATA section")?);
                append_text(&mut stack, data);
            } else if self.starts_with(b"<?") {
                self.pos += 2;
                self.skip_past(b"?>", "processing instruction")?;
            } else if self.starts_with(b"<!") {
                self.pos += 2;
                self.skip_past(b">", "declaration")?;
            } else if self.starts_with(b"</") {
                self.flush_text(&mut stack, &mut text)?;
                self.pos += 2;
                self.skip_past(b">", "end tag")?;
                if stack.len() == 1 {
                    return Err(self.error("end tag without an open element"));
                }
                let element = stack.pop().expect("open element");
                push(&mut stack, Node::Element(element));
            } else {
                self.flush_text(&mut stack, &mut text)?;
                self.pos += 1;
                let (element, empty) = self.start_tag()?;
                if empty {
                    push(&mut stack, Node::Element(element));
                } else {
                    stack.push(element);
                }
            }
        }
        self.flush_text(&mut stack, &mut text)?;
        if stack.len() > 1 {
            let open = stack.last().expect("open element").name.clone();
            return Err(self.error(&format!("element <{open}> not closed")));
        }
        let top = stack.pop().expect("top level");
        Ok(Document {
            nodes: top
                .children
                .into_iter()
                .filter(|n| matches!(n, Node::Element(_)))
                .collect(),
        })
    }

    fn flush_text(&self, stack: &mut [Element], text: &mut Vec<u8>) -> io::Result<()> {
        if !text.is_empty() {
            let decoded = decode_entities(text).map_err(|e| self.error(&e))?;
            text.clear();
            append_text(stack, decoded);
        }
        Ok(())
    }

    /// Reads a start tag after its `<`; returns the element and whether it
    /// was empty (`/>`).
    fn start_tag(&mut self) -> io::Result<(Element, bool)> {
        let name = self.name();
        if name.is_empty() {
            return Err(self.error("element without a name"));
        }
        let mut element = Element {
            name: cp1251::decode(name),
            ..Element::default()
        };
        loop {
            self.skip_space();
            match self.rest().first() {
                None => return Err(self.error("unterminated start tag")),
                Some(b'>') => {
                    self.pos += 1;
                    return Ok((element, false));
                }
                Some(b'/') if self.rest().get(1) == Some(&b'>') => {
                    self.pos += 2;
                    return Ok((element, true));
                }
                Some(b'/') => return Err(self.error("stray `/` in a start tag")),
                Some(_) => {
                    let name = self.attribute_name();
                    if name.is_empty() {
                        return Err(self.error("attribute without a name"));
                    }
                    let name = cp1251::decode(name);
                    self.skip_space();
                    let value = if self.rest().first() == Some(&b'=') {
                        self.pos += 1;
                        self.skip_space();
                        self.attribute_value()?
                    } else {
                        String::new()
                    };
                    element.attributes.push((name, value));
                }
            }
        }
    }

    fn name(&mut self) -> &[u8] {
        let start = self.pos;
        while let Some(&b) = self.text.get(self.pos)
            && !is_space_byte(b)
            && b != b'/'
            && b != b'>'
        {
            self.pos += 1;
        }
        &self.text[start..self.pos]
    }

    fn attribute_name(&mut self) -> &[u8] {
        let start = self.pos;
        while let Some(&b) = self.text.get(self.pos)
            && !is_space_byte(b)
            && !matches!(b, b'=' | b'/' | b'>')
        {
            self.pos += 1;
        }
        &self.text[start..self.pos]
    }

    fn attribute_value(&mut self) -> io::Result<String> {
        let raw = match self.rest().first() {
            Some(&q @ (b'"' | b'\'')) => {
                self.pos += 1;
                self.skip_past(&[q], "attribute value")?
            }
            _ => {
                let start = self.pos;
                while let Some(&b) = self.text.get(self.pos)
                    && !is_space_byte(b)
                    && b != b'/'
                    && b != b'>'
                {
                    self.pos += 1;
                }
                &self.text[start..self.pos]
            }
        };
        let raw = raw.to_vec();
        decode_entities(&raw).map_err(|e| self.error(&e))
    }
}

/// Appends `node` to the innermost open element.
fn push(stack: &mut [Element], node: Node) {
    stack.last_mut().expect("top level").children.push(node);
}

/// Appends decoded text to the innermost open element, merging with a text
/// node before it and dropping text that is only white space.
fn append_text(stack: &mut [Element], decoded: String) {
    let children = &mut stack.last_mut().expect("top level").children;
    if let Some(Node::Text(previous)) = children.last_mut() {
        previous.push_str(&decoded);
    } else if !decoded.chars().all(is_space) {
        children.push(Node::Text(decoded));
    }
}

/// Decodes Windows-1251 bytes with the five predefined entities and numeric
/// character references.
fn decode_entities(raw: &[u8]) -> Result<String, String> {
    if !raw.contains(&b'&') {
        return Ok(cp1251::decode(raw));
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.iter().position(|&b| b == b'&') {
        out.push_str(&cp1251::decode(&rest[..amp]));
        rest = &rest[amp..];
        let semicolon = rest.iter().take(12).position(|&b| b == b';');
        let entity = semicolon.and_then(|end| entity(&rest[1..end]).transpose());
        match entity {
            Some(c) => {
                out.push(c?);
                rest = &rest[semicolon.expect("entity end") + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(&cp1251::decode(rest));
    Ok(out)
}

/// The character of entity `name` (between `&` and `;`), `None` if unknown.
fn entity(name: &[u8]) -> Result<Option<char>, String> {
    let c = match name {
        b"amp" => '&',
        b"lt" => '<',
        b"gt" => '>',
        b"quot" => '"',
        b"apos" => '\'',
        [b'#', b'x' | b'X', hex @ ..] => return reference(hex, 16),
        [b'#', decimal @ ..] => return reference(decimal, 10),
        _ => return Ok(None),
    };
    Ok(Some(c))
}

/// A numeric character reference: a Unicode code point, which must exist in
/// Windows-1251 (text is used as Windows-1251 bytes).
fn reference(digits: &[u8], radix: u32) -> Result<Option<char>, String> {
    let Some(code) = std::str::from_utf8(digits)
        .ok()
        .filter(|d| !d.is_empty())
        .and_then(|d| u32::from_str_radix(d, radix).ok())
    else {
        return Ok(None);
    };
    match char::from_u32(code) {
        Some(c) if cp1251::encode_char(c).is_some() => Ok(Some(c)),
        _ => Err(format!(
            "character reference U+{code:04X} is not in Windows-1251"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        parse(&cp1251::encode(text).unwrap()).unwrap()
    }

    #[test]
    fn accepts_the_shipped_tolerances() {
        // Section 1.3: unquoted values, no separating space, `--` in comments,
        // a byte-order mark and declaration before the root.
        let d = doc(
            "\u{43f}\u{bb}\u{457}<?xml version='1.0' encoding=\"utf-8\"?>\n\
            <!-- ---------- -->\n\
            <window><main x=0 y=0 width=1024 height=768/>\
            <text  align=\"l\"x=\"0\" y='5' b=\"23\"complex_mode=\"0\"/>\
            <list item_height = \"70\"></list></window>",
        );
        let root = d.root().unwrap();
        assert_eq!(root.name, "window");
        let main = root.child("main").unwrap();
        assert_eq!(main.attr("height"), Some("768"));
        assert_eq!(main.attr("width"), Some("1024"));
        let text = root.child("text").unwrap();
        let attrs: Vec<(&str, &str)> = text
            .attributes
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .collect();
        assert_eq!(
            attrs,
            [
                ("align", "l"),
                ("x", "0"),
                ("y", "5"),
                ("b", "23"),
                ("complex_mode", "0")
            ]
        );
        assert_eq!(root.child("list").unwrap().attr("item_height"), Some("70"));
    }

    #[test]
    fn end_tag_closes_innermost_element() {
        let d = doc("<_window x=\"1\"><list/></window>");
        let root = d.root().unwrap();
        assert_eq!(root.name, "_window");
        assert_eq!(root.elements().count(), 1);
        assert!(parse(b"<a></a></b>").is_err());
        assert!(parse(b"<a><b></a>").is_err());
        assert!(parse(b"<a><!-- x").is_err());
    }

    #[test]
    fn values_and_entities() {
        let d = doc("<r><texture> ui\\ui_dg_info</texture><e/><m>\n<c/>x</m>\
            <t>a &amp; &lt;b&gt; &quot;&apos; &#1053;&#x41; &nbsp; <![CDATA[<&amp;>]]></t>\
            <s>  \u{a0}x\u{a0} </s></r>");
        let root = d.root().unwrap();
        assert_eq!(
            root.child("texture").unwrap().value(),
            Some("ui\\ui_dg_info")
        );
        assert_eq!(root.child("e").unwrap().value(), None);
        assert_eq!(root.child("m").unwrap().value(), None);
        assert_eq!(
            root.child("t").unwrap().value(),
            Some("a & <b> \"' \u{41d}A &nbsp; <&amp;>")
        );
        assert_eq!(root.child("s").unwrap().value(), Some("\u{a0}x\u{a0}"));
        assert!(parse(b"<t>&#200;</t>").is_err());
    }

    #[test]
    fn finds_paths_with_index() {
        let d = doc("<w><a n=\"0\"><b><c/></b></a><x/><a n=\"1\"><b><c v=\"2\"/></b></a></w>");
        assert_eq!(d.find("a", 1).unwrap().attr("n"), Some("1"));
        assert_eq!(d.find("a:b:c", 1).unwrap().attr("v"), Some("2"));
        assert!(d.find("a", 2).is_none());
        assert!(d.find("A", 0).is_none());
        assert_eq!(d.count("a"), 2);
    }

    #[test]
    fn preprocesses_includes() {
        let text = b"<r>\r\n\r\n#include \"ui\\a.xml\"\t\r\n  #include \"no.xml\"\n</r>";
        let mut seen = Vec::new();
        let out = preprocess(text, &mut |path| {
            seen.push(cp1251::decode(path));
            Ok(match path {
                b"ui\\a.xml" => b"<?xml version=\"1.0\"?>\r\n#include \"b.xml\"".to_vec(),
                _ => b"<b/>".to_vec(),
            })
        })
        .unwrap();
        assert_eq!(seen, ["ui\\a.xml", "b.xml"]);
        assert_eq!(
            out,
            b"<r>\n<?xml version=\"1.0\"?>\n<b/>\n  #include \"no.xml\"\n</r>\n"
        );
        let cycle = preprocess(b"#include \"x\"", &mut |_| Ok(b"#include \"x\"".to_vec()));
        assert!(cycle.is_err());
        assert!(preprocess(b"#include x", &mut |_| Ok(Vec::new())).is_err());
    }
}
