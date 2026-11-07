use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessTier {
    ReadOnly,
    Root,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub struct AccessContext {
    pub tier: AccessTier,
}

impl AccessContext {
    pub fn detect(force_read_only: bool) -> Self {
        let tier = if force_read_only {
            AccessTier::ReadOnly
        } else if effective_uid() == 0 {
            AccessTier::Root
        } else {
            AccessTier::ReadOnly
        };
        Self { tier }
    }

    #[allow(dead_code)]
    pub fn allows_mutation(&self) -> bool {
        matches!(self.tier, AccessTier::Root)
    }
}

pub fn effective_uid() -> u32 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(parse_uid_line)
        .unwrap_or(1000)
}

fn parse_uid_line(source: String) -> Option<u32> {
    source
        .lines()
        .find(|line| line.starts_with("Uid:"))
        .and_then(|line| line.split_whitespace().nth(2))
        .and_then(|value| value.parse().ok())
}

pub fn default_log_file() -> PathBuf {
    if effective_uid() == 0 {
        PathBuf::from("/var/log/anvil/anvil.log")
    } else if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        PathBuf::from(xdg).join("anvil/anvil.log")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".local/state/anvil/anvil.log")
    } else {
        PathBuf::from("/tmp/anvil.log")
    }
}
