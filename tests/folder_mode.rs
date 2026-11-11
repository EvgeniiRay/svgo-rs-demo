//! Folder mode (`-f`) must produce, for every input file, exactly the
//! bytes of a single-file `optimize_preset` run, regardless of how
//! the parallel work is scheduled.

#[test]
fn folder_mode_matches_single_file_outputs() {
    let bin = env!("CARGO_BIN_EXE_svgo-rs");
    let base = std::env::temp_dir().join(format!("svgo-rs-folder-test-{}", std::process::id()));
    let input_dir = base.join("in");
    let out_dir = base.join("out");
    std::fs::create_dir_all(&input_dir).unwrap();
    let svg = "<svg width=\"24\" height=\"24\" viewBox=\"0 0 24 24\" fill=\"none\" xmlns=\"http://www.w3.org/2000/svg\">\n<path d=\"M10.5 5C10.5 5.64117 10.9022 6.18838 11.4682 6.403C11.8174 6.48797 12.1822 6.48801 12.5315 6.4031C13.0975 6.18855 13.5 5.64127 13.5 5C13.5 4.17157 12.8284 3.5 12 3.5C11.1715 3.5 10.5 4.17157 10.5 5Z\" fill=\"#212121\"/>\n</svg>\n";
    let mut names = Vec::new();
    for i in 0..32 {
        let name = format!("icon{i:02}.svg");
        std::fs::write(input_dir.join(&name), svg).unwrap();
        names.push(name);
    }
    let status = std::process::Command::new(bin)
        .args(["-f", input_dir.to_str().unwrap(), "-o", out_dir.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(status.success(), "folder mode failed: {status}");
    let expected = svgo_rs::optimize_preset(svg).unwrap();
    for name in &names {
        let got = std::fs::read_to_string(out_dir.join(name)).unwrap();
        assert_eq!(got, expected, "folder output differs for {name}");
    }
    let _ = std::fs::remove_dir_all(&base);
}
