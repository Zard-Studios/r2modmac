fn main() {
    let patterns = vec![
        "/Users/federicofeduzi/Applications/Steambuild 32 64bit DXVK.app/Contents/SharedSupport/prefix/drive_c/Program Files (x86)/Steam/steamapps/common/PEAK/PEAK.exe".to_string(),
        "C:\\Program Files (x86)\\Steam\\steamapps\\common\\PEAK\\PEAK.exe".to_string(),
        "PEAK.exe".to_string(),
        "peak.exe".to_string(),
    ];
    for pattern in patterns {
        let plain = pattern.replace(r"\/", "/").replace(r"\.", ".");
        let status = std::process::Command::new("pgrep")
            .arg("-fl")
            .arg(&plain)
            .output()
            .unwrap();
        if status.status.success() {
            println!("MATCHED {:?}: \n{}", plain, String::from_utf8_lossy(&status.stdout));
        } else {
            println!("No match for {:?}", plain);
        }
    }
}
