//! SteamID tests ported from SteamKit2's SteamIDFacts.cs.

use steam::types::SteamId;

#[test]
fn from_u64_extracts_components() {
    // SteamID 103582791432294076 = Valve clan group
    let sid = SteamId::new(103582791432294076);
    assert_eq!(sid.account_id(), 2772668);
    assert_eq!(sid.universe(), 1); // Public
    assert_eq!(sid.account_type(), 7); // Clan
}

#[test]
fn from_u64_game_server() {
    // SteamID 157626004137848889
    let sid = SteamId::new(157626004137848889);
    assert_eq!(sid.account_id(), 12345);
    assert_eq!(sid.universe(), 2); // Beta
    assert_eq!(sid.account_type(), 3); // GameServer
    assert_eq!(sid.raw(), 157626004137848889);
}

#[test]
fn from_parts_round_trips() {
    let sid = SteamId::from_parts(1, 1, 1, 12345678);
    assert_eq!(sid.universe(), 1);
    assert_eq!(sid.account_type(), 1);
    assert_eq!(sid.instance(), 1);
    assert_eq!(sid.account_id(), 12345678);

    // Reconstruct and verify raw value is stable
    let sid2 = SteamId::new(sid.raw());
    assert_eq!(sid, sid2);
}

#[test]
fn display_steam3_individual() {
    let sid = SteamId::from_parts(1, 1, 1, 123);
    assert_eq!(sid.to_string(), "[U:1:123]");
}

#[test]
fn display_steam3_clan() {
    let sid = SteamId::from_parts(1, 7, 0, 2772668);
    assert_eq!(sid.to_string(), "[g:1:2772668]");
}
