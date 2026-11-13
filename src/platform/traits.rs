use std::io;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub trait TextReader: Send + Sync {
    fn read_to_string(&self, path: &Path) -> Result<String, io::Error>;
    fn read_dir_names(&self, path: &Path) -> Result<Vec<String>, io::Error>;
    fn exists(&self, path: &Path) -> bool;
}

#[derive(Debug, Default)]
pub struct OsTextReader;

impl TextReader for OsTextReader {
    fn read_to_string(&self, path: &Path) -> Result<String, io::Error> {
        std::fs::read_to_string(path)
    }

    fn read_dir_names(&self, path: &Path) -> Result<Vec<String>, io::Error> {
        let mut names = std::fs::read_dir(path)?
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &Path, args: &[&str]) -> Result<CommandOutput, io::Error>;
}

#[derive(Debug, Default)]
pub struct OsCommandRunner;

impl CommandRunner for OsCommandRunner {
    fn run(&self, program: &Path, args: &[&str]) -> Result<CommandOutput, io::Error> {
        let output = std::process::Command::new(program).args(args).output()?;
        Ok(CommandOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        })
    }
}
