// Common types
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A flexible JSON object (equivalent to TypeScript's LooseObject)
pub type LooseObject = HashMap<String, serde_json::Value>;

/// Standard API response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRes {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

impl BaseRes {
    /// Create a successful response
    pub fn ok() -> Self {
        Self {
            ok: true,
            msg: None,
        }
    }

    /// Create a successful response with a message
    pub fn ok_with_msg(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            msg: Some(msg.into()),
        }
    }

    /// Create an error response
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            msg: Some(msg.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_res_ok() {
        let res = BaseRes::ok();
        assert!(res.ok);
        assert!(res.msg.is_none());
    }

    #[test]
    fn test_base_res_ok_with_msg() {
        let res = BaseRes::ok_with_msg("Success");
        assert!(res.ok);
        assert_eq!(res.msg, Some("Success".to_string()));
    }

    #[test]
    fn test_base_res_error() {
        let res = BaseRes::error("Something went wrong");
        assert!(!res.ok);
        assert_eq!(res.msg, Some("Something went wrong".to_string()));
    }

    #[test]
    fn test_base_res_serialization() {
        let res = BaseRes::ok_with_msg("Test");
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"msg\":\"Test\""));
    }

    #[test]
    fn test_loose_object() {
        let mut obj = LooseObject::new();
        obj.insert("key1".to_string(), serde_json::json!("value1"));
        obj.insert("key2".to_string(), serde_json::json!(42));
        obj.insert("nested".to_string(), serde_json::json!({"a": 1, "b": 2}));
        
        assert_eq!(obj.get("key1"), Some(&serde_json::json!("value1")));
        assert_eq!(obj.get("key2"), Some(&serde_json::json!(42)));
    }
}
