//! Replay test: verify we correctly parse the encryption handshake
//! from a real captured session.
//!
//! This test replays the first two packets (ChannelEncryptRequest and
//! ChannelEncryptResult) from a live capture and verifies:
//! - The ChannelEncryptRequest is parsed with correct EMsg
//! - The protocol version and universe are readable
//! - The challenge bytes are present

use steam::messages::EMsg;
use steam::messages::header::MsgHdr;
use steam::transport::capture::CaptureFile;

const CAPTURES: &str = "tests/captures";

fn load_capture(name: &str) -> CaptureFile {
    CaptureFile::load(std::path::Path::new(&format!("{CAPTURES}/{name}")))
        .expect("capture file should exist")
}

#[test]
fn parse_channel_encrypt_request_from_capture() {
    let capture = load_capture("depots_480.json");

    // First packet should be ChannelEncryptRequest (unencrypted)
    let pkt = &capture.packets[0];
    assert_eq!(pkt.emsg, Some(1303), "first packet should be ChannelEncryptRequest");

    let payload = pkt.decode_payload().expect("valid base64");

    // Parse the MsgHdr
    let mut reader = payload.as_slice();
    let hdr = MsgHdr::parse(&mut reader).expect("should parse MsgHdr");

    assert_eq!(hdr.emsg, EMsg::CHANNEL_ENCRYPT_REQUEST);

    // After the header: protocol_version (u32) + universe (u32) + challenge bytes
    assert!(reader.len() >= 8, "should have protocol_version + universe");

    let protocol_version = u32::from_le_bytes([reader[0], reader[1], reader[2], reader[3]]);
    let universe = u32::from_le_bytes([reader[4], reader[5], reader[6], reader[7]]);

    assert_eq!(protocol_version, 1, "protocol version should be 1");
    assert_eq!(universe, 1, "universe should be Public (1)");

    // Remaining bytes after protocol_version + universe are the challenge
    let challenge = &reader[8..];
    assert!(challenge.len() >= 16, "challenge should be at least 16 bytes, got {}", challenge.len());
}

#[test]
fn parse_channel_encrypt_result_from_capture() {
    let capture = load_capture("depots_480.json");

    // Second packet should be ChannelEncryptResult (unencrypted)
    let pkt = &capture.packets[1];
    assert_eq!(pkt.emsg, Some(1305), "second packet should be ChannelEncryptResult");

    let payload = pkt.decode_payload().expect("valid base64");

    let mut reader = payload.as_slice();
    let hdr = MsgHdr::parse(&mut reader).expect("should parse MsgHdr");

    assert_eq!(hdr.emsg, EMsg::CHANNEL_ENCRYPT_RESULT);

    // After header: EResult (u32)
    assert!(reader.len() >= 4, "should have EResult");
    let eresult = u32::from_le_bytes([reader[0], reader[1], reader[2], reader[3]]);
    assert_eq!(eresult, 1, "EResult should be OK (1)");
}

#[test]
fn capture_has_expected_packet_count() {
    let depots = load_capture("depots_480.json");
    assert!(depots.packets.len() >= 4, "depots capture should have at least 4 packets (handshake + logon response + PICS)");

    let manifests = load_capture("manifests_223350_223351.json");
    assert!(manifests.packets.len() >= 4, "manifests capture should have at least 4 packets");
}
