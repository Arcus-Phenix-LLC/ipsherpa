use std::path::PathBuf;
#[cfg(not(test))]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn path() -> PathBuf {
    if let Some(directory) = std::env::var_os("IPSHERPA_LOG_DIR") {
        return PathBuf::from(directory).join("ipsherpa.log");
    }
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("IPSherpa").join("logs").join("ipsherpa.log")
}

#[cfg(not(test))]
pub fn log(message: &str) {
    let path = path();
    let result = (|| -> std::io::Result<()> {
        fs::create_dir_all(path.parent().expect("log directory"))?;
        let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        writeln!(file, "[{seconds} pid={}] {message}", std::process::id())?;
        file.sync_data()
    })();
    if let Err(error) = result {
        eprintln!("Unable to write {}: {error}", path.display());
    }
}

#[cfg(test)]
pub fn log(_message: &str) {}

pub fn init() {
    log(concat!(
        "IPSherpa GUI ",
        env!("CARGO_PKG_VERSION"),
        " starting"
    ));
    std::panic::set_hook(Box::new(|info| {
        let message = format!("Rust panic: {info}");
        log(&message);
        eprintln!("{message}");
    }));
}
