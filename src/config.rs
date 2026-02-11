use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

/// Dockge - A fancy, easy-to-use and reactive self-hosted docker compose.yaml stack manager
#[derive(Parser, Debug)]
#[command(name = "dockru")]
#[command(version, about, long_about = None)]
pub struct Config {
    /// Path to SSL key file
    #[arg(long, env = "DOCKGE_SSL_KEY")]
    pub ssl_key: Option<PathBuf>,

    /// Path to SSL certificate file
    #[arg(long, env = "DOCKGE_SSL_CERT")]
    pub ssl_cert: Option<PathBuf>,

    /// SSL key passphrase
    #[arg(long, env = "DOCKGE_SSL_KEY_PASSPHRASE")]
    pub ssl_key_passphrase: Option<String>,

    /// Port to listen on
    #[arg(short, long, env = "DOCKGE_PORT", default_value = "5001")]
    pub port: u16,

    /// Hostname to bind to
    #[arg(long, env = "DOCKGE_HOSTNAME")]
    pub hostname: Option<String>,

    /// Data directory for application data
    #[arg(long, env = "DOCKGE_DATA_DIR", default_value = "./data")]
    pub data_dir: PathBuf,

    /// Stacks directory for docker compose files
    #[arg(long, env = "DOCKGE_STACKS_DIR")]
    #[cfg_attr(target_os = "windows", arg(default_value = "./stacks"))]
    #[cfg_attr(not(target_os = "windows"), arg(default_value = "/opt/stacks"))]
    pub stacks_dir: PathBuf,

    /// Enable interactive console
    #[arg(long, env = "DOCKGE_ENABLE_CONSOLE", default_value = "false")]
    pub enable_console: bool,
}

impl Config {
    pub fn parse() -> Result<Self> {
        let config = <Self as Parser>::parse();
        Ok(config)
    }

    /// Check if SSL is configured
    pub fn is_ssl_enabled(&self) -> bool {
        self.ssl_key.is_some() && self.ssl_cert.is_some()
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
