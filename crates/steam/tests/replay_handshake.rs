//! Replay tests: verify we correctly parse captured server packets.
//!
//! These tests use real captures from anonymous login sessions against
//! various Steam apps. Each capture has 6 packets:
//!   0: ChannelEncryptRequest (unencrypted)
//!   1: ChannelEncryptResult  (unencrypted)
//!   2-5: Encrypted session packets (logon response, PICS responses, etc.)

use steam::messages::EMsg;
use steam::messages::header::MsgHdr;
use steam::transport::capture::CaptureFile;

const CAPTURES: &str = "tests/captures";

fn load_capture(name: &str) -> CaptureFile {
    CaptureFile::load(std::path::Path::new(&format!("{CAPTURES}/{name}")))
        .unwrap_or_else(|e| panic!("failed to load capture {name}: {e}"))
}

/// All capture files we have.
fn all_captures() -> Vec<(&'static str, CaptureFile)> {
    let names = [
        "depots_480.json",
        "depots_223350.json",
        "depots_570.json",
        "depots_730.json",
        "manifests_480_481.json",
        "manifests_223350_223351.json",
        "files_480_481.json",
        "files_223350_223351.json",
        "files_1874900_1874901.json",
    ];
    names
        .iter()
        .map(|name| (*name, load_capture(name)))
        .collect()
}

#[test]
fn all_captures_have_valid_encrypt_request() {
    for (name, capture) in all_captures() {
        let pkt = &capture.packets[0];
        assert_eq!(
            pkt.emsg,
            Some(1303),
            "{name}: first packet should be ChannelEncryptRequest"
        );

        let payload = pkt.decode_payload().unwrap();
        let mut reader = payload.as_slice();
        let hdr = MsgHdr::parse(&mut reader)
            .unwrap_or_else(|e| panic!("{name}: failed to parse MsgHdr: {e}"));

        assert_eq!(hdr.emsg, EMsg::CHANNEL_ENCRYPT_REQUEST, "{name}");

        // protocol_version + universe
        assert!(
            reader.len() >= 8,
            "{name}: not enough bytes for version+universe, got {}",
            reader.len()
        );

        let protocol_version = u32::from_le_bytes([reader[0], reader[1], reader[2], reader[3]]);
        let universe = u32::from_le_bytes([reader[4], reader[5], reader[6], reader[7]]);

        assert_eq!(protocol_version, 1, "{name}: protocol version");
        assert_eq!(universe, 1, "{name}: universe (Public)");

        // Challenge bytes
        let challenge = &reader[8..];
        assert!(
            challenge.len() >= 16,
            "{name}: challenge too short: {} bytes",
            challenge.len()
        );
    }
}

#[test]
fn all_captures_have_valid_encrypt_result() {
    for (name, capture) in all_captures() {
        let pkt = &capture.packets[1];
        assert_eq!(
            pkt.emsg,
            Some(1305),
            "{name}: second packet should be ChannelEncryptResult"
        );

        let payload = pkt.decode_payload().unwrap();
        let mut reader = payload.as_slice();
        let hdr = MsgHdr::parse(&mut reader)
            .unwrap_or_else(|e| panic!("{name}: failed to parse MsgHdr: {e}"));

        assert_eq!(hdr.emsg, EMsg::CHANNEL_ENCRYPT_RESULT, "{name}");

        assert!(reader.len() >= 4, "{name}: not enough bytes for EResult");
        let eresult = u32::from_le_bytes([reader[0], reader[1], reader[2], reader[3]]);
        assert_eq!(eresult, 1, "{name}: EResult should be OK");
    }
}

#[test]
fn all_captures_have_consistent_structure() {
    for (name, capture) in all_captures() {
        assert!(
            capture.packets.len() >= 4,
            "{name}: expected at least 4 packets (handshake + logon), got {}",
            capture.packets.len()
        );

        // Packet sequence numbers should be monotonic
        for (i, pkt) in capture.packets.iter().enumerate() {
            assert_eq!(pkt.seq, i as u32, "{name}: packet {i} has wrong seq");
        }

        // First two are unencrypted (known EMsgs), rest are encrypted (EMsgs will look like garbage)
        assert_eq!(
            capture.packets[0].emsg,
            Some(1303),
            "{name}: pkt0 = EncryptRequest"
        );
        assert_eq!(
            capture.packets[1].emsg,
            Some(1305),
            "{name}: pkt1 = EncryptResult"
        );

        // All packets after the handshake should have non-zero payloads
        for i in 2..capture.packets.len() {
            let payload = capture.packets[i].decode_payload().unwrap();
            assert!(!payload.is_empty(), "{name}: packet {i} is empty");
        }
    }
}

#[test]
fn capture_file_round_trips_through_json() {
    let original = load_capture("depots_480.json");

    let tmp = std::env::temp_dir().join("depotdownloader_test_roundtrip.json");
    original.save(&tmp).expect("save");

    let reloaded = CaptureFile::load(&tmp).expect("load");
    std::fs::remove_file(&tmp).ok();

    assert_eq!(original.packets.len(), reloaded.packets.len());
    for (a, b) in original.packets.iter().zip(reloaded.packets.iter()) {
        assert_eq!(a.seq, b.seq);
        assert_eq!(a.emsg, b.emsg);
        assert_eq!(a.payload_b64, b.payload_b64);
    }
}

#[tokio::test]
async fn replay_transport_serves_packets_in_order() {
    use steam::transport::Transport;
    use steam::transport::replay::ReplayTransport;

    let capture = load_capture("depots_480.json");
    let replay = ReplayTransport::from_capture(&capture).unwrap();

    // Should get all 6 packets in order
    for (i, expected) in capture.packets.iter().enumerate() {
        let payload = replay.recv().await.unwrap();
        let expected_payload = expected.decode_payload().unwrap();
        assert_eq!(
            payload.as_ref(),
            expected_payload.as_slice(),
            "packet {i} mismatch"
        );
    }

    // Next recv should return Disconnected
    let result = replay.recv().await;
    assert!(result.is_err(), "should error after all packets consumed");
}

#[tokio::test]
async fn replay_transport_discards_sends() {
    use steam::transport::Transport;
    use steam::transport::replay::ReplayTransport;

    let capture = load_capture("depots_480.json");
    let replay = ReplayTransport::from_capture(&capture).unwrap();

    // Sends should succeed silently
    replay.send(b"hello").await.unwrap();
    replay.send(b"world").await.unwrap();

    // Recv should still work and return packet 0 (sends don't consume)
    let payload = replay.recv().await.unwrap();
    let expected = capture.packets[0].decode_payload().unwrap();
    assert_eq!(payload.as_ref(), expected.as_slice());
}
