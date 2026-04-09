//! Valve's binary KeyValue format parser.
//!
//! Used for PICS product info responses. The format is a type-tagged
//! tree where each node has a null-terminated key name and a value
//! determined by the type byte.

use std::collections::BTreeMap;

use winnow::binary::{le_f32, le_i32, le_i64, le_u64, le_u8};
use winnow::error::{ContextError, ErrMode, StrContext, StrContextValue};
use winnow::token::take_until;
use winnow::{ModalResult, Parser};

/// Type tag byte in the binary KV format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum KvTag {
    /// Section header (has children, no inline value).
    None = 0x00,
    String = 0x01,
    Int32 = 0x02,
    Float32 = 0x03,
    Pointer = 0x04,
    WideString = 0x05,
    Color = 0x06,
    UInt64 = 0x07,
    /// Section terminator.
    End = 0x08,
    Int64 = 0x0A,
    /// Alternative section terminator (treated identically to End).
    AltEnd = 0x0B,
}

impl KvTag {
    pub fn from_u8(b: u8) -> Option<Self> {
        match b {
            0x00 => Some(Self::None),
            0x01 => Some(Self::String),
            0x02 => Some(Self::Int32),
            0x03 => Some(Self::Float32),
            0x04 => Some(Self::Pointer),
            0x05 => Some(Self::WideString),
            0x06 => Some(Self::Color),
            0x07 => Some(Self::UInt64),
            0x08 => Some(Self::End),
            0x0A => Some(Self::Int64),
            0x0B => Some(Self::AltEnd),
            _ => Option::None,
        }
    }

    pub fn is_end(self) -> bool {
        matches!(self, Self::End | Self::AltEnd)
    }
}

/// A value in a KeyValue tree.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum KvValue {
    String(String),
    Int32(i32),
    Float32(f32),
    UInt64(u64),
    Int64(i64),
    Color(i32),
    Pointer(i32),
    /// A section containing child key-value pairs.
    Children(BTreeMap<String, KeyValue>),
}

/// A single node in a KeyValue tree.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct KeyValue {
    pub key: String,
    pub value: KvValue,
}

impl KeyValue {
    /// Look up a child by key (case-insensitive, matching Valve's behavior).
    pub fn get(&self, key: &str) -> Option<&KeyValue> {
        match &self.value {
            KvValue::Children(children) => children
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(key))
                .map(|(_, v)| v),
            _ => Option::None,
        }
    }

    /// Get the string value, if this is a string node.
    pub fn as_str(&self) -> Option<&str> {
        match &self.value {
            KvValue::String(s) => Some(s),
            _ => Option::None,
        }
    }

    /// Get the i32 value.
    pub fn as_i32(&self) -> Option<i32> {
        match &self.value {
            KvValue::Int32(v) => Some(*v),
            _ => Option::None,
        }
    }

    /// Get the u64 value.
    pub fn as_u64(&self) -> Option<u64> {
        match &self.value {
            KvValue::UInt64(v) => Some(*v),
            _ => Option::None,
        }
    }
}

/// Parse a binary KeyValue tree from a byte slice.
///
/// This is the top-level entry point. The root node is read as a single
/// type-tagged entry.
pub fn parse_binary_kv(input: &mut &[u8]) -> ModalResult<KeyValue> {
    parse_node(input)
}

/// Parse a single KV node (type byte + key + value).
fn parse_node(input: &mut &[u8]) -> ModalResult<KeyValue> {
    let type_byte = le_u8
        .context(StrContext::Label("kv type tag"))
        .parse_next(input)?;

    let tag = KvTag::from_u8(type_byte).ok_or_else(|| {
        let mut err = ContextError::new();
        err.push(StrContext::Expected(StrContextValue::Description(
            "valid KV type tag (0x00-0x08, 0x0A, 0x0B)",
        )));
        err.push(StrContext::Label("kv type tag"));
        ErrMode::Cut(err)
    })?;

    let key = parse_null_string
        .context(StrContext::Label("kv key name"))
        .parse_next(input)?;

    let value = match tag {
        KvTag::None => KvValue::Children(parse_children(input)?),
        KvTag::String => KvValue::String(
            parse_null_string
                .context(StrContext::Label("kv string value"))
                .parse_next(input)?,
        ),
        KvTag::Int32 => KvValue::Int32(
            le_i32
                .context(StrContext::Label("kv int32 value"))
                .parse_next(input)?,
        ),
        KvTag::Float32 => KvValue::Float32(
            le_f32
                .context(StrContext::Label("kv float32 value"))
                .parse_next(input)?,
        ),
        KvTag::Pointer => KvValue::Pointer(
            le_i32
                .context(StrContext::Label("kv pointer value"))
                .parse_next(input)?,
        ),
        KvTag::Color => KvValue::Color(
            le_i32
                .context(StrContext::Label("kv color value"))
                .parse_next(input)?,
        ),
        KvTag::UInt64 => KvValue::UInt64(
            le_u64
                .context(StrContext::Label("kv uint64 value"))
                .parse_next(input)?,
        ),
        KvTag::Int64 => KvValue::Int64(
            le_i64
                .context(StrContext::Label("kv int64 value"))
                .parse_next(input)?,
        ),
        KvTag::WideString => KvValue::String(String::new()), // unsupported
        KvTag::End | KvTag::AltEnd => {
            let mut err = ContextError::new();
            err.push(StrContext::Expected(StrContextValue::Description(
                "value node, not section terminator",
            )));
            err.push(StrContext::Label("kv node type"));
            return Err(ErrMode::Cut(err));
        }
    };

    Ok(KeyValue { key, value })
}

/// Parse children until we hit an End or AltEnd tag.
fn parse_children(input: &mut &[u8]) -> ModalResult<BTreeMap<String, KeyValue>> {
    let mut children = BTreeMap::new();
    loop {
        if input.is_empty() {
            break;
        }
        let tag = KvTag::from_u8(input[0]);
        if tag.is_some_and(|t| t.is_end()) {
            // Consume the end marker
            le_u8.parse_next(input)?;
            break;
        }
        let node = parse_node(input)?;
        children.insert(node.key.clone(), node);
    }
    Ok(children)
}

/// Parse a null-terminated UTF-8 string.
fn parse_null_string(input: &mut &[u8]) -> ModalResult<String> {
    let bytes: &[u8] = take_until(0.., b'\0')
        .context(StrContext::Label("null-terminated string"))
        .parse_next(input)?;
    // Consume the null terminator
    le_u8.parse_next(input)?;
    Ok(String::from_utf8_lossy(bytes).into_owned())
}


/// Parse a text-format KeyValue string (used by PICS app info responses).
///
/// Format:
/// ```text
/// "key"
/// {
///     "child_key"     "string_value"
///     "section_key"
///     {
///         ...
///     }
/// }
/// ```
pub fn parse_text_kv(input: &str) -> Result<KeyValue, TextKvError> {
    let mut chars = input.trim();
    parse_text_node(&mut chars)
}

/// Errors from text KV parsing.
#[derive(Debug, thiserror::Error)]
pub enum TextKvError {
    #[error("unexpected end of input")]
    UnexpectedEof,
    #[error("expected '\"' to start a quoted string")]
    ExpectedQuote,
    #[error("expected '{{' to open a section")]
    ExpectedOpenBrace,
}

fn parse_text_node(input: &mut &str) -> Result<KeyValue, TextKvError> {
    skip_whitespace_and_comments(input);
    let key = parse_quoted_string(input)?;
    skip_whitespace_and_comments(input);

    if input.starts_with('{') {
        *input = &input[1..];
        let children = parse_text_children(input)?;
        Ok(KeyValue {
            key,
            value: KvValue::Children(children),
        })
    } else if input.starts_with('"') {
        let value = parse_quoted_string(input)?;
        Ok(KeyValue {
            key,
            value: KvValue::String(value),
        })
    } else {
        Err(TextKvError::UnexpectedEof)
    }
}

fn parse_text_children(input: &mut &str) -> Result<BTreeMap<String, KeyValue>, TextKvError> {
    let mut children = BTreeMap::new();
    loop {
        skip_whitespace_and_comments(input);
        if input.is_empty() {
            break;
        }
        if input.starts_with('}') {
            *input = &input[1..];
            break;
        }
        let node = parse_text_node(input)?;
        children.insert(node.key.clone(), node);
    }
    Ok(children)
}

fn parse_quoted_string(input: &mut &str) -> Result<String, TextKvError> {
    skip_whitespace_and_comments(input);
    if !input.starts_with('"') {
        return Err(TextKvError::ExpectedQuote);
    }
    *input = &input[1..]; // skip opening quote

    let mut result = String::new();
    let mut chars = input.chars();
    loop {
        match chars.next() {
            Some('"') => break,
            Some('\\') => {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some(c) => {
                        result.push('\\');
                        result.push(c);
                    }
                    None => return Err(TextKvError::UnexpectedEof),
                }
            }
            Some(c) => result.push(c),
            None => return Err(TextKvError::UnexpectedEof),
        }
    }
    *input = chars.as_str();
    Ok(result)
}

fn skip_whitespace_and_comments(input: &mut &str) {
    loop {
        *input = input.trim_start();
        if input.starts_with("//") {
            // Skip to end of line
            if let Some(idx) = input.find('\n') {
                *input = &input[idx + 1..];
            } else {
                *input = "";
            }
        } else {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a simple binary KV for testing.
    fn make_test_kv() -> Vec<u8> {
        let mut buf = Vec::new();
        // Root section: type=None, key="root"
        buf.push(KvTag::None as u8);
        buf.extend_from_slice(b"root\0");

        // Child string
        buf.push(KvTag::String as u8);
        buf.extend_from_slice(b"name\0");
        buf.extend_from_slice(b"Test App\0");

        // Child int32
        buf.push(KvTag::Int32 as u8);
        buf.extend_from_slice(b"appid\0");
        buf.extend_from_slice(&480i32.to_le_bytes());

        // Child uint64
        buf.push(KvTag::UInt64 as u8);
        buf.extend_from_slice(b"size\0");
        buf.extend_from_slice(&12345u64.to_le_bytes());

        // Nested section
        buf.push(KvTag::None as u8);
        buf.extend_from_slice(b"depots\0");
        buf.push(KvTag::String as u8);
        buf.extend_from_slice(b"480\0");
        buf.extend_from_slice(b"depot_data\0");
        buf.push(KvTag::End as u8); // end depots

        buf.push(KvTag::End as u8); // end root
        buf
    }

    #[test]
    fn parse_basic_kv() {
        let data = make_test_kv();
        let mut input = data.as_slice();
        let kv = parse_binary_kv(&mut input).unwrap();

        assert_eq!(kv.key, "root");

        let name = kv.get("name").unwrap();
        assert_eq!(name.as_str(), Some("Test App"));

        let appid = kv.get("appid").unwrap();
        assert_eq!(appid.as_i32(), Some(480));

        let size = kv.get("size").unwrap();
        assert_eq!(size.as_u64(), Some(12345));
    }

    #[test]
    fn parse_nested_kv() {
        let data = make_test_kv();
        let mut input = data.as_slice();
        let kv = parse_binary_kv(&mut input).unwrap();

        let depots = kv.get("depots").unwrap();
        let depot_480 = depots.get("480").unwrap();
        assert_eq!(depot_480.as_str(), Some("depot_data"));
    }

    #[test]
    fn input_fully_consumed() {
        let data = make_test_kv();
        let mut input = data.as_slice();
        let _kv = parse_binary_kv(&mut input).unwrap();
        assert!(input.is_empty());
    }

    #[test]
    fn invalid_tag_gives_useful_error() {
        let data = [0xFF, b'x', 0x00]; // 0xFF is not a valid tag
        let mut input = data.as_slice();
        let err = parse_binary_kv(&mut input).unwrap_err();

        let msg = format!("{err}");
        assert!(
            msg.contains("valid KV type tag"),
            "error should mention valid tag types, got: {msg}"
        );
    }
}
