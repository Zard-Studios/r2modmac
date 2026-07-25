use std::path::PathBuf;
use tauri::Manager;

pub fn app_data_dir<R: tauri::Runtime>(manager: &impl Manager<R>) -> Result<PathBuf, tauri::Error> {
    let dir = manager.path().app_data_dir()?;
    #[cfg(target_os = "windows")]
    {
        let mut dir = dir;
        if dir.file_name().map(|f| f == "com.r2modmac").unwrap_or(false) {
            dir.pop();
            dir.push("r2modmac");
        }
        return Ok(dir);
    }
    #[cfg(not(target_os = "windows"))]
    Ok(dir)
}

pub fn app_cache_dir<R: tauri::Runtime>(manager: &impl Manager<R>) -> Result<PathBuf, tauri::Error> {
    let dir = manager.path().app_cache_dir()?;
    #[cfg(target_os = "windows")]
    {
        let mut dir = dir;
        if dir.file_name().map(|f| f == "com.r2modmac").unwrap_or(false) {
            dir.pop();
            dir.push("r2modmac");
        }
        return Ok(dir);
    }
    #[cfg(not(target_os = "windows"))]
    Ok(dir)
}
