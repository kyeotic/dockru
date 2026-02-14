// Constants and status codes

// Stack Status
pub const UNKNOWN: i32 = 0;
pub const CREATED_FILE: i32 = 1;
pub const CREATED_STACK: i32 = 2;
pub const RUNNING: i32 = 3;
pub const EXITED: i32 = 4;

// Terminal dimensions
pub const TERMINAL_COLS: u16 = 105;
pub const TERMINAL_ROWS: u16 = 10;
pub const PROGRESS_TERMINAL_ROWS: u16 = 8;
pub const COMBINED_TERMINAL_COLS: u16 = 58;
pub const COMBINED_TERMINAL_ROWS: u16 = 20;

// Error types
#[allow(dead_code)]
pub const ERROR_TYPE_VALIDATION: i32 = 1;

// Special endpoint marker
pub const ALL_ENDPOINTS: &str = "##ALL_DOCKRU_ENDPOINTS##";

// Accepted compose file names (in order of preference)
pub const ACCEPTED_COMPOSE_FILE_NAMES: &[&str] = &[
    "compose.yaml",
    "docker-compose.yaml",
    "docker-compose.yml",
    "compose.yml",
];

/// Convert status code to status name
#[allow(dead_code)]
pub fn status_name(status: i32) -> &'static str {
    match status {
        CREATED_FILE => "draft",
        CREATED_STACK => "created_stack",
        RUNNING => "running",
        EXITED => "exited",
        _ => "unknown",
    }
}

/// Convert status code to short status name
#[allow(dead_code)]
pub fn status_name_short(status: i32) -> &'static str {
    match status {
        CREATED_FILE => "inactive",
        CREATED_STACK => "inactive",
        RUNNING => "active",
        EXITED => "exited",
        _ => "?",
    }
}

/// Get color for status code
#[allow(dead_code)]
pub fn status_color(status: i32) -> &'static str {
    match status {
        CREATED_FILE => "dark",
        CREATED_STACK => "dark",
        RUNNING => "primary",
        EXITED => "danger",
        _ => "secondary",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_name() {
        assert_eq!(status_name(RUNNING), "running");
        assert_eq!(status_name(EXITED), "exited");
        assert_eq!(status_name(CREATED_FILE), "draft");
        assert_eq!(status_name(UNKNOWN), "unknown");
    }

    #[test]
    fn test_status_name_short() {
        assert_eq!(status_name_short(RUNNING), "active");
        assert_eq!(status_name_short(EXITED), "exited");
        assert_eq!(status_name_short(CREATED_FILE), "inactive");
    }

    #[test]
    fn test_status_color() {
        assert_eq!(status_color(RUNNING), "primary");
        assert_eq!(status_color(EXITED), "danger");
        assert_eq!(status_color(CREATED_FILE), "dark");
    }
}
