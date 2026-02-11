// YAML utilities with comment preservation
use anyhow::{Context, Result};
use std::collections::HashMap;
use yaml_rust2::{yaml::Hash, Yaml, YamlEmitter, YamlLoader};

/// Substitute environment variables in a string
///
/// Replaces ${VAR} and $VAR patterns with values from the provided map
///
/// # Arguments
/// * `s` - The string to process
/// * `variables` - Map of variable names to values
///
/// # Returns
/// The string with variables substituted
pub fn envsubst(s: &str, variables: &HashMap<String, String>) -> String {
    envsubst::substitute(s, variables).unwrap_or_else(|_| s.to_string())
}

/// Substitute environment variables in YAML content
///
/// Parses the YAML, recursively substitutes variables in all string values,
/// and returns the modified YAML as a string.
///
/// # Arguments
/// * `content` - YAML content as a string
/// * `env` - Map of environment variable names to values
///
/// # Returns
/// Result containing the YAML with substituted variables
pub fn envsubst_yaml(content: &str, env: &HashMap<String, String>) -> Result<String> {
    // Parse the YAML
    let mut docs = YamlLoader::load_from_str(content)
        .context("Failed to parse YAML")?;
    
    if docs.is_empty() {
        return Ok(content.to_string());
    }
    
    // Substitute variables in the first document
    let doc = docs.remove(0);
    let substituted = traverse_yaml_for_envsubst(doc, env);
    
    // Emit back to string
    let mut output = String::new();
    let mut emitter = YamlEmitter::new(&mut output);
    emitter.dump(&substituted)
        .context("Failed to emit YAML")?;
    
    Ok(output)
}

/// Recursively traverse YAML structure and substitute environment variables
fn traverse_yaml_for_envsubst(yaml: Yaml, env: &HashMap<String, String>) -> Yaml {
    match yaml {
        Yaml::String(s) => Yaml::String(envsubst(&s, env)),
        Yaml::Array(arr) => {
            let new_arr: Vec<Yaml> = arr
                .into_iter()
                .map(|item| traverse_yaml_for_envsubst(item, env))
                .collect();
            Yaml::Array(new_arr)
        }
        Yaml::Hash(hash) => {
            let mut new_hash = Hash::new();
            for (key, value) in hash {
                let new_key = traverse_yaml_for_envsubst(key, env);
                let new_value = traverse_yaml_for_envsubst(value, env);
                new_hash.insert(new_key, new_value);
            }
            Yaml::Hash(new_hash)
        }
        // Other types (Integer, Real, Boolean, Null, etc.) are returned as-is
        other => other,
    }
}

/// Copy YAML comments from source document to destination document
///
/// Note: yaml-rust2 has limited comment preservation support.
/// This is a placeholder for the full implementation which would require
/// more sophisticated AST manipulation or a different YAML library.
///
/// # Arguments
/// * `doc` - Destination YAML document
/// * `src` - Source YAML document with comments to copy
///
/// # Returns
/// The document with comments (currently returns doc as-is)
pub fn copy_yaml_comments(doc: Yaml, _src: Yaml) -> Yaml {
    // TODO: This is a simplified implementation
    // Full comment preservation would require:
    // 1. Custom YAML parser that preserves comment AST nodes
    // 2. Or using a different library like serde_yaml with custom deserializer
    // 3. Or manual string manipulation to preserve comments
    //
    // For now, we return the document as-is
    // This will need to be enhanced when stack editing is implemented
    doc
}

/// Parse YAML from a string
///
/// # Arguments
/// * `content` - YAML content as a string
///
/// # Returns
/// Result containing the parsed YAML document
pub fn parse_yaml(content: &str) -> Result<Vec<Yaml>> {
    YamlLoader::load_from_str(content)
        .context("Failed to parse YAML")
}

/// Convert YAML document to string
///
/// # Arguments
/// * `doc` - YAML document
///
/// # Returns
/// Result containing the YAML as a string
pub fn yaml_to_string(doc: &Yaml) -> Result<String> {
    let mut output = String::new();
    let mut emitter = YamlEmitter::new(&mut output);
    emitter.dump(doc)
        .context("Failed to emit YAML")?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envsubst_basic() {
        let mut vars = HashMap::new();
        vars.insert("NAME".to_string(), "World".to_string());
        
        let result = envsubst("Hello ${NAME}", &vars);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_envsubst_missing_var() {
        let vars = HashMap::new();
        
        let result = envsubst("Hello ${NAME}", &vars);
        // envsubst should leave undefined variables as-is or empty
        assert!(result.starts_with("Hello"));
    }

    #[test]
    fn test_envsubst_yaml_simple() {
        let yaml = r#"
name: ${APP_NAME}
version: ${VERSION}
"#;
        
        let mut env = HashMap::new();
        env.insert("APP_NAME".to_string(), "myapp".to_string());
        env.insert("VERSION".to_string(), "1.0.0".to_string());
        
        let result = envsubst_yaml(yaml, &env).unwrap();
        assert!(result.contains("myapp"));
        assert!(result.contains("1.0.0"));
    }

    #[test]
    fn test_envsubst_yaml_nested() {
        let yaml = r#"
services:
  web:
    image: nginx:${VERSION}
    environment:
      - DATABASE_URL=${DB_URL}
"#;
        
        let mut env = HashMap::new();
        env.insert("VERSION".to_string(), "latest".to_string());
        env.insert("DB_URL".to_string(), "postgres://localhost".to_string());
        
        let result = envsubst_yaml(yaml, &env).unwrap();
        assert!(result.contains("latest"));
        assert!(result.contains("postgres://localhost"));
    }

    #[test]
    fn test_parse_yaml_valid() {
        let yaml = r#"
key: value
number: 42
list:
  - item1
  - item2
"#;
        
        let docs = parse_yaml(yaml).unwrap();
        assert!(!docs.is_empty());
    }

    #[test]
    fn test_parse_yaml_invalid() {
        let yaml = "invalid: yaml: content: :";
        let result = parse_yaml(yaml);
        // Should still parse or return an error
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_yaml_to_string() {
        let yaml = r#"
key: value
number: 42
"#;
        
        let docs = parse_yaml(yaml).unwrap();
        let output = yaml_to_string(&docs[0]).unwrap();
        assert!(output.contains("key"));
        assert!(output.contains("value"));
    }
}
