use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Dockru - A fancy, easy-to-use and reactive self-hosted docker compose.yaml stack manager
#[derive(Parser, Debug)]
#[command(name = "dockru")]
#[command(version, about, long_about = None)]
pub struct Config {
    /// Port to listen on
    #[arg(short, long, env = "DOCKRU_PORT", default_value = "5001")]
    pub port: u16,

    /// Hostname to bind to
    #[arg(long, env = "DOCKRU_HOSTNAME")]
    pub hostname: Option<String>,

    /// Data directory for application data
    #[arg(long, env = "DOCKRU_DATA_DIR", default_value = "./data")]
    pub data_dir: PathBuf,

    /// Stacks directory for docker compose files
    #[arg(long, env = "DOCKRU_STACKS_DIR")]
    #[cfg_attr(target_os = "windows", arg(default_value = "./stacks"))]
    #[cfg_attr(not(target_os = "windows"), arg(default_value = "/opt/stacks"))]
    pub stacks_dir: PathBuf,

    /// Enable interactive console
    #[arg(long, env = "DOCKRU_ENABLE_CONSOLE", default_value = "false")]
    pub enable_console: bool,
}

impl Config {
    pub fn parse() -> Result<Self> {
        let config = <Self as Parser>::parse();
        Ok(config)
    }

    /// Get the bind address as a string
    pub fn bind_address(&self) -> String {
        if let Some(ref hostname) = self.hostname {
            format!("{}:{}", hostname, self.port)
        } else {
            format!("0.0.0.0:{}", self.port)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        // Just test that we can parse with no args
        let config = Config::parse();
        assert!(config.is_ok());
    }
}
