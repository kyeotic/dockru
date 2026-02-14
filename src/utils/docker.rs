// Docker-related utilities
use serde::{Deserialize, Serialize};

/// Parsed Docker port information
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockerPort {
    pub url: String,
    pub display: String,
}

/// Parse a Docker port specification into a URL and display string
///
/// Supports various formats:
/// - "3000"
/// - "3000-3005"
/// - "8000:8000"
/// - "9090-9091:8080-8081"
/// - "49100:22"
/// - "8000-9000:80"
/// - "127.0.0.1:8001:8001"
/// - "127.0.0.1:5000-5010:5000-5010"
/// - "0.0.0.0:8080->8080/tcp"
/// - "6060:6060/udp"
///
/// # Arguments
/// * `input` - The port specification string
/// * `hostname` - The hostname to use in the URL
///
/// # Returns
/// A DockerPort struct with the URL and display string
pub fn parse_docker_port(input: &str, hostname: &str) -> DockerPort {
    let port: String;
    let display: String;
    let mut protocol = String::from("tcp");
    let mut host = hostname.to_string();

    // Split protocol (tcp/udp)
    let parts: Vec<&str> = input.split('/').collect();
    let mut part1 = parts[0].to_string();
    if parts.len() > 1 {
        protocol = parts[1].to_string();
    }

    // Handle docker ps output format with arrow (->)
    if let Some(arrow_pos) = part1.find("->") {
        part1 = part1[..arrow_pos].to_string();
        if let Some(colon_pos) = part1.find(':') {
            part1 = part1[(colon_pos + 1)..].to_string();
        }
    }

    // Find the last colon to separate host mapping from container port
    if let Some(last_colon) = part1.rfind(':') {
        let host_part = part1[..last_colon].to_string();
        display = host_part.clone();

        // Check if there's a dash (port range) in the FULL part1
        // This matches TypeScript behavior: dash check is on part1, not hostPart
        let port_for_url = if let Some(dash_pos) = part1.find('-') {
            part1[..dash_pos].to_string()
        } else {
            host_part.clone()
        };

        // Check if there's an IP address (ip:port format)
        if let Some(colon_pos) = port_for_url.find(':') {
            host = port_for_url[..colon_pos].to_string();
            port = port_for_url[(colon_pos + 1)..].to_string();
        } else {
            port = port_for_url;
        }
    } else {
        // No colon, so it's just a port or port range
        if let Some(dash_pos) = part1.find('-') {
            // Port range: use the first port
            port = part1[..dash_pos].to_string();
        } else {
            // Just a port
            port = part1.clone();
        }
        display = part1;
    }

    // Parse port as integer
    let port_int: u16 = port.parse().unwrap_or(80);

    // Determine the URL protocol
    let url_protocol = if port_int == 443 {
        "https"
    } else if protocol == "tcp" {
        "http"
    } else {
        &protocol
    };

    let url = format!("{}://{}:{}", url_protocol, host, port_int);

    DockerPort { url, display }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_docker_port_simple() {
        let result = parse_docker_port("3000", "localhost");
        assert_eq!(result.url, "http://localhost:3000");
        assert_eq!(result.display, "3000");
    }

    #[test]
    fn test_parse_docker_port_range() {
        let result = parse_docker_port("3000-3005", "localhost");
        assert_eq!(result.url, "http://localhost:3000");
        assert_eq!(result.display, "3000-3005");
    }

    #[test]
    fn test_parse_docker_port_mapping() {
        let result = parse_docker_port("8000:8000", "localhost");
        assert_eq!(result.url, "http://localhost:8000");
        assert_eq!(result.display, "8000");
    }

    #[test]
    fn test_parse_docker_port_different_mapping() {
        let result = parse_docker_port("49100:22", "localhost");
        assert_eq!(result.url, "http://localhost:49100");
        assert_eq!(result.display, "49100");
    }

    #[test]
    fn test_parse_docker_port_range_mapping() {
        let result = parse_docker_port("9090-9091:8080-8081", "localhost");
        assert_eq!(result.url, "http://localhost:9090");
        assert_eq!(result.display, "9090-9091");
    }

    #[test]
    fn test_parse_docker_port_with_ip() {
        let result = parse_docker_port("127.0.0.1:8001:8001", "localhost");
        assert_eq!(result.url, "http://127.0.0.1:8001");
        assert_eq!(result.display, "127.0.0.1:8001");
    }

    #[test]
    fn test_parse_docker_port_with_ip_range() {
        let result = parse_docker_port("127.0.0.1:5000-5010:5000-5010", "localhost");
        assert_eq!(result.url, "http://127.0.0.1:5000");
        assert_eq!(result.display, "127.0.0.1:5000-5010");
    }

    #[test]
    fn test_parse_docker_port_arrow_format() {
        let result = parse_docker_port("0.0.0.0:8080->8080/tcp", "localhost");
        assert_eq!(result.url, "http://localhost:8080");
        assert_eq!(result.display, "8080");
    }

    #[test]
    fn test_parse_docker_port_udp() {
        let result = parse_docker_port("6060:6060/udp", "localhost");
        assert_eq!(result.url, "udp://localhost:6060");
        assert_eq!(result.display, "6060");
    }

    #[test]
    fn test_parse_docker_port_https() {
        let result = parse_docker_port("443:443", "localhost");
        assert_eq!(result.url, "https://localhost:443");
        assert_eq!(result.display, "443");
    }
}
