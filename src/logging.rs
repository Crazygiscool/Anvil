use crate::error::Result;
use std::io;
use std::path::Path;
use tracing_subscriber::EnvFilter;

pub fn init(log_file: Option<&Path>) -> Result<()> {
    let file_path = log_file.map(Path::to_path_buf);
    let make_writer = move || {
        if let Some(path) = &file_path {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map(|file| Box::new(file) as Box<dyn std::io::Write + Send>)
                .unwrap_or_else(|_| Box::new(io::sink()))
        } else {
            Box::new(io::sink())
        }
    };

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("anvil=info"));
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(make_writer)
        .with_target(true)
        .with_ansi(false)
        .finish();

    let _ = tracing::subscriber::set_global_default(subscriber);
    Ok(())
}
