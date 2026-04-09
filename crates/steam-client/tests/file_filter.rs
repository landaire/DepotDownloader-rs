use steam_client::download::FileFilter;

#[test]
fn filelist_matches_case_insensitive() {
    let filter = FileFilter::FileList(vec![
        "bin/game.exe".to_string(),
        "data/config.cfg".to_string(),
    ]);

    assert!(filter.matches("bin/game.exe"));
    assert!(filter.matches("BIN/GAME.EXE"));
    assert!(filter.matches("Bin/Game.Exe"));
    assert!(!filter.matches("bin/other.exe"));
}

#[test]
fn filelist_normalizes_backslashes() {
    let filter = FileFilter::FileList(vec!["bin/game.exe".to_string()]);

    assert!(filter.matches("bin\\game.exe"));
}

#[test]
fn regex_matches_pattern() {
    let filter = FileFilter::from_regex(r"\.exe$").unwrap();

    assert!(filter.matches("bin/game.exe"));
    assert!(filter.matches("SteamworksExample.exe"));
    assert!(!filter.matches("data/config.cfg"));
}

#[test]
fn regex_is_case_insensitive() {
    let filter = FileFilter::from_regex(r"readme").unwrap();

    assert!(filter.matches("README.txt"));
    assert!(filter.matches("Readme.md"));
    assert!(filter.matches("docs/readme"));
}

#[test]
fn filelist_from_file() {
    let dir = std::env::temp_dir().join("dd_test_filelist");
    std::fs::create_dir_all(&dir).ok();
    let path = dir.join("files.txt");
    std::fs::write(&path, "bin/game.exe\ndata/config.cfg\n").unwrap();

    let filter = FileFilter::from_filelist(&path).unwrap();
    assert!(filter.matches("bin/game.exe"));
    assert!(filter.matches("data/config.cfg"));
    assert!(!filter.matches("other.txt"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn empty_filelist_matches_nothing() {
    let filter = FileFilter::FileList(vec![]);
    assert!(!filter.matches("anything.txt"));
}
