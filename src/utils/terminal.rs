// Terminal naming utilities

/// Get the name for a compose terminal
///
/// # Arguments
/// * `endpoint` - The endpoint identifier
/// * `stack` - The stack name
///
/// # Returns
/// Terminal name in format "compose-{endpoint}-{stack}"
pub fn get_compose_terminal_name(endpoint: &str, stack: &str) -> String {
    format!("compose-{}-{}", endpoint, stack)
}

/// Get the name for a combined terminal
///
/// # Arguments
/// * `endpoint` - The endpoint identifier
/// * `stack` - The stack name
///
/// # Returns
/// Terminal name in format "combined-{endpoint}-{stack}"
pub fn get_combined_terminal_name(endpoint: &str, stack: &str) -> String {
    format!("combined-{}-{}", endpoint, stack)
}

/// Get the name for a container terminal
///
/// # Arguments
/// * `endpoint` - The endpoint identifier
/// * `container` - The container name
///
/// # Returns
/// Terminal name in format "container-{endpoint}-{container}"
pub fn get_container_terminal_name(endpoint: &str, container: &str) -> String {
    format!("container-{}-{}", endpoint, container)
}

/// Get the name for a container exec terminal
///
/// # Arguments
/// * `endpoint` - The endpoint identifier
/// * `stack_name` - The stack name
/// * `container` - The container name
/// * `index` - The instance index
///
/// # Returns
/// Terminal name in format "container-exec-{endpoint}-{stack_name}-{container}-{index}"
pub fn get_container_exec_terminal_name(
    endpoint: &str,
    stack_name: &str,
    container: &str,
    index: usize,
) -> String {
    format!(
        "container-exec-{}-{}-{}-{}",
        endpoint, stack_name, container, index
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_compose_terminal_name() {
        assert_eq!(
            get_compose_terminal_name("localhost", "mystack"),
            "compose-localhost-mystack"
        );
    }

    #[test]
    fn test_get_combined_terminal_name() {
        assert_eq!(
            get_combined_terminal_name("localhost", "mystack"),
            "combined-localhost-mystack"
        );
    }

    #[test]
    fn test_get_container_terminal_name() {
        assert_eq!(
            get_container_terminal_name("localhost", "my-container"),
            "container-localhost-my-container"
        );
    }

    #[test]
    fn test_get_container_exec_terminal_name() {
        assert_eq!(
            get_container_exec_terminal_name("localhost", "mystack", "my-container", 0),
            "container-exec-localhost-mystack-my-container-0"
        );
        assert_eq!(
            get_container_exec_terminal_name("remote", "stack1", "web", 5),
            "container-exec-remote-stack1-web-5"
        );
    }
}
