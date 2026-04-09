//! Snapshot tests for Valve's binary KeyValue format parser.

use steam::types::key_value::{KvTag, KvValue, parse_binary_kv};

/// Build a KV with various value types for comprehensive snapshot coverage.
fn make_all_types_kv() -> Vec<u8> {
    let mut buf = Vec::new();
    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"appinfo\0");

    buf.push(KvTag::String as u8);
    buf.extend_from_slice(b"name\0");
    buf.extend_from_slice(b"Team Fortress 2\0");

    buf.push(KvTag::Int32 as u8);
    buf.extend_from_slice(b"appid\0");
    buf.extend_from_slice(&440i32.to_le_bytes());

    buf.push(KvTag::UInt64 as u8);
    buf.extend_from_slice(b"size\0");
    buf.extend_from_slice(&15_000_000_000u64.to_le_bytes());

    buf.push(KvTag::Float32 as u8);
    buf.extend_from_slice(b"score\0");
    buf.extend_from_slice(&3.14f32.to_le_bytes());

    buf.push(KvTag::Int64 as u8);
    buf.extend_from_slice(b"timestamp\0");
    buf.extend_from_slice(&(-1234567890i64).to_le_bytes());

    buf.push(KvTag::Color as u8);
    buf.extend_from_slice(b"bg_color\0");
    buf.extend_from_slice(&0x00FF00FFi32.to_le_bytes());

    // Nested section
    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"depots\0");

    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"441\0");
    buf.push(KvTag::String as u8);
    buf.extend_from_slice(b"name\0");
    buf.extend_from_slice(b"Team Fortress 2 Content\0");
    buf.push(KvTag::End as u8); // end 441

    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"440\0");
    buf.push(KvTag::String as u8);
    buf.extend_from_slice(b"name\0");
    buf.extend_from_slice(b"Team Fortress 2 Client\0");
    buf.push(KvTag::End as u8); // end 440

    buf.push(KvTag::End as u8); // end depots
    buf.push(KvTag::End as u8); // end root
    buf
}

#[test]
fn all_value_types() {
    let data = make_all_types_kv();
    let mut input = data.as_slice();
    let kv = parse_binary_kv(&mut input).expect("should parse");
    assert!(input.is_empty(), "should consume all input");
    insta::assert_toml_snapshot!(kv);
}

#[test]
fn empty_section() {
    let mut buf = Vec::new();
    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"empty\0");
    buf.push(KvTag::End as u8);

    let mut input = buf.as_slice();
    let kv = parse_binary_kv(&mut input).expect("should parse");
    insta::assert_toml_snapshot!(kv);
}

#[test]
fn nested_lookup() {
    let data = make_all_types_kv();
    let mut input = data.as_slice();
    let kv = parse_binary_kv(&mut input).unwrap();

    // Case-insensitive lookup
    let depots = kv.get("DEPOTS").expect("should find depots case-insensitively");
    let d441 = depots.get("441").expect("should find depot 441");
    assert_eq!(d441.get("name").and_then(|n| n.as_str()), Some("Team Fortress 2 Content"));
}

#[test]
fn alt_end_marker() {
    let mut buf = Vec::new();
    buf.push(KvTag::None as u8);
    buf.extend_from_slice(b"root\0");
    buf.push(KvTag::String as u8);
    buf.extend_from_slice(b"key\0");
    buf.extend_from_slice(b"value\0");
    buf.push(KvTag::AltEnd as u8); // 0x0B instead of 0x08

    let mut input = buf.as_slice();
    let kv = parse_binary_kv(&mut input).expect("should accept alt end marker");
    assert_eq!(kv.get("key").and_then(|k| k.as_str()), Some("value"));
}

#[test]
fn invalid_tag_errors() {
    let data = [0xFF, b'x', 0x00];
    let mut input = data.as_slice();
    assert!(parse_binary_kv(&mut input).is_err());
}
