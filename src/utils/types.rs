// Common types
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A flexible JSON object (equivalent to TypeScript's LooseObject)
#[allow(dead_code)]
pub type LooseObject = HashMap<String, serde_json::Value>;

/// Standard API response structure
///
/// # Examples
///
/// ```
/// use dockru::utils::types::BaseRes;
///
/// // Simple success
/// let res = BaseRes::ok();
///
/// // Success with message
/// let res = BaseRes::ok_with_msg("Operation completed");
///
/// // Success with i18n message
/// let res = BaseRes::ok_with_msg_i18n("operationComplete");
///
/// // Success with data
/// let res = BaseRes::ok_with_data(json!({"count": 42}));
///
/// // Error response
/// let res = BaseRes::error("Something went wrong");
///
/// // Error with i18n key
/// let res = BaseRes::error_i18n("errorKey");
///
/// // Builder pattern
/// let res = BaseRes::ok()
///     .with_data(json!({"value": 123}))
///     .with_i18n();
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseRes {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msgi18n: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl BaseRes {
    /// Create a successful response
    pub fn ok() -> Self {
        Self {
            ok: true,
            msg: None,
            msgi18n: None,
            data: None,
        }
    }

    /// Create a successful response with a message
    pub fn ok_with_msg(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            msg: Some(msg.into()),
            msgi18n: None,
            data: None,
        }
    }

    /// Create a successful response with an i18n message key
    pub fn ok_with_msg_i18n(msg: impl Into<String>) -> Self {
        Self {
            ok: true,
            msg: Some(msg.into()),
            msgi18n: Some(true),
            data: None,
        }
    }

    /// Create a successful response with data
    pub fn ok_with_data<T: Serialize>(data: T) -> Self {
        Self {
            ok: true,
            msg: None,
            msgi18n: None,
            data: serde_json::to_value(data).ok(),
        }
    }

    /// Create an error response
    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            msg: Some(msg.into()),
            msgi18n: None,
            data: None,
        }
    }

    /// Create an error response with an i18n message key
    pub fn error_i18n(msg: impl Into<String>) -> Self {
        Self {
            ok: false,
            msg: Some(msg.into()),
            msgi18n: Some(true),
            data: None,
        }
    }

    /// Add data to an existing response (builder pattern)
    pub fn with_data<T: Serialize>(mut self, data: T) -> Self {
        self.data = serde_json::to_value(data).ok();
        self
    }

    /// Mark the message as i18n (builder pattern)
    pub fn with_i18n(mut self) -> Self {
        self.msgi18n = Some(true);
        self
    }
}

/// Convert BaseRes to serde_json::Value for compatibility with existing code
impl From<BaseRes> for serde_json::Value {
    fn from(res: BaseRes) -> Self {
        serde_json::to_value(res).expect("BaseRes serialization should never fail")
    }
}

/// Generic response with custom fields
///
/// Use this for responses that need additional fields beyond the standard BaseRes.
///
/// # Examples
///
/// ```
/// use dockru::utils::types::{BaseRes, CustomResponse};
/// use serde::Serialize;
///
/// #[derive(Serialize)]
/// struct LoginResponse {
///     token: String,
/// }
///
/// let response = CustomResponse::ok_with_fields(LoginResponse {
///     token: "jwt-token-here".to_string(),
/// });
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomResponse<T> {
    #[serde(flatten)]
    pub base: BaseRes,
    #[serde(flatten)]
    pub fields: T,
}

impl<T: Serialize> CustomResponse<T> {
    /// Create a successful response with custom fields
    pub fn ok_with_fields(fields: T) -> Self {
        Self {
            base: BaseRes::ok(),
            fields,
        }
    }

    /// Create an error response with custom fields
    pub fn error_with_fields(msg: impl Into<String>, fields: T) -> Self {
        Self {
            base: BaseRes::error(msg),
            fields,
        }
    }
}

/// Convert CustomResponse to serde_json::Value
impl<T: Serialize> From<CustomResponse<T>> for serde_json::Value {
    fn from(res: CustomResponse<T>) -> Self {
        serde_json::to_value(res).expect("CustomResponse serialization should never fail")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_base_res_ok() {
        let res = BaseRes::ok();
        assert!(res.ok);
        assert!(res.msg.is_none());
        assert!(res.msgi18n.is_none());
        assert!(res.data.is_none());
    }

    #[test]
    fn test_base_res_ok_with_msg() {
        let res = BaseRes::ok_with_msg("Success");
        assert!(res.ok);
        assert_eq!(res.msg, Some("Success".to_string()));
        assert!(res.msgi18n.is_none());
    }

    #[test]
    fn test_base_res_ok_with_msg_i18n() {
        let res = BaseRes::ok_with_msg_i18n("successKey");
        assert!(res.ok);
        assert_eq!(res.msg, Some("successKey".to_string()));
        assert_eq!(res.msgi18n, Some(true));
    }

    #[test]
    fn test_base_res_ok_with_data() {
        let data = json!({"count": 42, "items": ["a", "b"]});
        let res = BaseRes::ok_with_data(&data);
        assert!(res.ok);
        assert_eq!(res.data, Some(data));
    }

    #[test]
    fn test_base_res_error() {
        let res = BaseRes::error("Something went wrong");
        assert!(!res.ok);
        assert_eq!(res.msg, Some("Something went wrong".to_string()));
        assert!(res.msgi18n.is_none());
    }

    #[test]
    fn test_base_res_error_i18n() {
        let res = BaseRes::error_i18n("errorKey");
        assert!(!res.ok);
        assert_eq!(res.msg, Some("errorKey".to_string()));
        assert_eq!(res.msgi18n, Some(true));
    }

    #[test]
    fn test_base_res_with_data_builder() {
        let res = BaseRes::ok().with_data(json!({"value": 123}));
        assert!(res.ok);
        assert_eq!(res.data, Some(json!({"value": 123})));
    }

    #[test]
    fn test_base_res_with_i18n_builder() {
        let res = BaseRes::ok_with_msg("key").with_i18n();
        assert!(res.ok);
        assert_eq!(res.msgi18n, Some(true));
    }

    #[test]
    fn test_base_res_builder_chain() {
        let res = BaseRes::ok()
            .with_data(json!({"test": true}))
            .with_i18n();
        assert!(res.ok);
        assert_eq!(res.data, Some(json!({"test": true})));
        assert_eq!(res.msgi18n, Some(true));
    }

    #[test]
    fn test_base_res_serialization() {
        let res = BaseRes::ok_with_msg("Test");
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"msg\":\"Test\""));
    }

    #[test]
    fn test_base_res_serialization_omits_none() {
        let res = BaseRes::ok();
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(!json.contains("\"msg\""));
        assert!(!json.contains("\"msgi18n\""));
        assert!(!json.contains("\"data\""));
    }

    #[test]
    fn test_base_res_serialization_with_data_and_i18n() {
        let res = BaseRes::ok_with_msg_i18n("key")
            .with_data(json!({"count": 5}));
        let json = serde_json::to_string(&res).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"msg\":\"key\""));
        assert!(json.contains("\"msgi18n\":true"));
        assert!(json.contains("\"data\":{\"count\":5}"));
    }

    #[test]
    fn test_base_res_to_value_conversion() {
        let res = BaseRes::ok_with_msg("Test");
        let value: serde_json::Value = res.into();
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["msg"], json!("Test"));
    }

    #[test]
    fn test_custom_response() {
        #[derive(Serialize)]
        struct LoginFields {
            token: String,
        }

        let response = CustomResponse::ok_with_fields(LoginFields {
            token: "test-token".to_string(),
        });

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"token\":\"test-token\""));
    }

    #[test]
    fn test_custom_response_with_error() {
        #[derive(Serialize)]
        struct ErrorFields {
            code: i32,
        }

        let response = CustomResponse::error_with_fields(
            "Error occurred",
            ErrorFields { code: 404 }
        );

        assert!(!response.base.ok);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"ok\":false"));
        assert!(json.contains("\"msg\":\"Error occurred\""));
        assert!(json.contains("\"code\":404"));
    }

    #[test]
    fn test_custom_response_to_value_conversion() {
        #[derive(Serialize)]
        struct TestFields {
            value: i32,
        }

        let response = CustomResponse::ok_with_fields(TestFields { value: 42 });
        let value: serde_json::Value = response.into();
        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["value"], json!(42));
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
