use std::env;
use std::path::PathBuf;

/// Platform-specific user data directory for tentoku.
pub fn get_user_data_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        let home = dirs_home();
        home.join("Library")
            .join("Application Support")
            .join("tentoku")
    }
    #[cfg(target_os = "windows")]
    {
        let appdata =
            env::var("APPDATA").unwrap_or_else(|_| dirs_home().to_string_lossy().into_owned());
        PathBuf::from(appdata).join("tentoku")
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let base = env::var("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs_home().join(".local").join("share"));
        base.join("tentoku")
    }
}

fn dirs_home() -> PathBuf {
    env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Default path to `jmdict.db`.
pub fn get_default_database_path() -> PathBuf {
    let dir = get_user_data_dir();
    let _ = std::fs::create_dir_all(&dir);
    dir.join("jmdict.db")
}

/// Search common locations for an existing database file.
pub fn find_database_path() -> Option<PathBuf> {
    // 1. Environment variable override
    if let Ok(path) = env::var("TENTOKU_DB") {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // 2. User data directory
    let user_path = get_user_data_dir().join("jmdict.db");
    if user_path.exists() {
        return Some(user_path);
    }

    // 3. Current directory
    let local = PathBuf::from("jmdict.db");
    if local.exists() {
        return Some(local);
    }

    None
}
