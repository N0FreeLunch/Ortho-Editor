//! Standardized 422 Unprocessable Entity validation response models.
//! Follows RFC 7807 / Laravel-style field-indexed violation arrays with actionable user guidance.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::HashMap;

/// Numeric or value constraints applicable to the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ConstraintRange {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<u64>,
}

impl ConstraintRange {
    pub fn new(min: impl Into<Option<u64>>, max: impl Into<Option<u64>>) -> Self {
        Self {
            min: min.into(),
            max: max.into(),
        }
    }
}

/// Detailed violation for an individual field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldViolation {
    /// Machine-readable violation code (e.g. "overflow", "underflow", "length", "invalid_range").
    pub code: Cow<'static, str>,

    /// Developer-readable description of the error.
    pub message: String,

    /// Concrete, actionable guide for the user on how to resolve the error.
    pub hint: String,

    /// The specific value or delta that caused the rejection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejected_value: Option<serde_json::Value>,

    /// Boundary constraints for the field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint: Option<ConstraintRange>,
}

impl FieldViolation {
    pub fn new(
        code: impl Into<Cow<'static, str>>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: hint.into(),
            rejected_value: None,
            constraint: None,
        }
    }

    pub fn with_rejected_value(mut self, val: impl Serialize) -> Self {
        self.rejected_value = serde_json::to_value(val).ok();
        self
    }

    pub fn with_constraint(
        mut self,
        min: impl Into<Option<u64>>,
        max: impl Into<Option<u64>>,
    ) -> Self {
        self.constraint = Some(ConstraintRange::new(min, max));
        self
    }
}

/// Standardized 422 Unprocessable Entity response payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ValidationErrorResponse {
    /// High level status indicator.
    pub status: &'static str,

    /// HTTP/IPC semantic status code (422 = Unprocessable Entity).
    pub code: u16,

    /// Summary message.
    pub message: &'static str,

    /// Map of field names to list of violations.
    pub errors: HashMap<String, Vec<FieldViolation>>,
}

impl Default for ValidationErrorResponse {
    fn default() -> Self {
        Self {
            status: "validation_error",
            code: 422,
            message: "The given data was invalid.",
            errors: HashMap::new(),
        }
    }
}

impl ValidationErrorResponse {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a violation to the given field.
    pub fn add_violation(&mut self, field: impl Into<String>, violation: FieldViolation) {
        self.errors.entry(field.into()).or_default().push(violation);
    }

    /// Convenience builder for single field violation.
    pub fn single(field: impl Into<String>, violation: FieldViolation) -> Self {
        let mut resp = Self::new();
        resp.add_violation(field, violation);
        resp
    }

    /// Returns true if there are any validation errors.
    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

impl From<validator::ValidationErrors> for ValidationErrorResponse {
    fn from(errors: validator::ValidationErrors) -> Self {
        let mut resp = Self::new();
        for (field, err_kind) in errors.into_errors() {
            match err_kind {
                validator::ValidationErrorsKind::Struct(sub_errors) => {
                    let sub_resp = ValidationErrorResponse::from(*sub_errors);
                    for (sub_field, sub_violations) in sub_resp.errors {
                        for v in sub_violations {
                            resp.add_violation(format!("{}.{}", field, sub_field), v);
                        }
                    }
                }
                validator::ValidationErrorsKind::List(list_errors) => {
                    for (idx, item_errors) in list_errors {
                        let sub_resp = ValidationErrorResponse::from(*item_errors);
                        for (sub_field, sub_violations) in sub_resp.errors {
                            for v in sub_violations {
                                resp.add_violation(format!("{}[{}].{}", field, idx, sub_field), v);
                            }
                        }
                    }
                }
                validator::ValidationErrorsKind::Field(field_errors) => {
                    for fe in field_errors {
                        let message = fe
                            .message
                            .map(|m| m.to_string())
                            .unwrap_or_else(|| format!("Invalid field value: {}", fe.code));
                        let hint = match fe.code.as_ref() {
                            "length" => "Ensure the character length satisfies the minimum and maximum boundaries.".to_string(),
                            "range" => "Ensure the value falls within the allowed minimum and maximum range.".to_string(),
                            other => format!("Ensure the value satisfies the '{}' rule.", other),
                        };

                        let mut violation = FieldViolation::new(fe.code, message, hint);
                        if let Some(param) = fe.params.get("value") {
                            violation.rejected_value = serde_json::to_value(param).ok();
                        }
                        let min_val = fe.params.get("min").and_then(|v| v.as_u64());
                        let max_val = fe.params.get("max").and_then(|v| v.as_u64());
                        if min_val.is_some() || max_val.is_some() {
                            violation.constraint = Some(ConstraintRange::new(min_val, max_val));
                        }
                        resp.add_violation(field, violation);
                    }
                }
            }
        }
        resp
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn violation_code_owns_runtime_json_and_borrows_static_literals() {
        let input = format!(
            r#"{{"code":"{}","message":"Rejected","hint":"Try again"}}"#,
            "custom.runtime"
        );
        let violation: FieldViolation = serde_json::from_str(&input).unwrap();
        drop(input);
        assert_eq!(violation.code, "custom.runtime");
        assert!(matches!(&violation.code, Cow::Owned(_)));
        assert_eq!(
            serde_json::to_value(violation).unwrap(),
            serde_json::json!({"code": "custom.runtime", "message": "Rejected", "hint": "Try again"})
        );
        let literal = FieldViolation::new("range", "Rejected", "Try again");
        assert!(matches!(&literal.code, Cow::Borrowed("range")));
    }

    #[test]
    fn validator_conversion_transfers_owned_codes_to_response() {
        let mut error = validator::ValidationError::new("custom");
        error.code = Cow::Owned(format!("custom.{}", "runtime"));
        let mut errors = validator::ValidationErrors::new();
        errors.add("quantity", error);
        let response = ValidationErrorResponse::from(errors);
        let violation = &response.errors["quantity"][0];
        assert_eq!(violation.code, "custom.runtime");
        assert!(matches!(&violation.code, Cow::Owned(_)));
    }

    #[test]
    fn nested_validator_errors_preserve_wire_fields_and_paths() {
        use validator::{ValidationError, ValidationErrors, ValidationErrorsKind};

        let mut range = ValidationError::new("range");
        range.add_param("value".into(), &0_u64);
        range.add_param("min".into(), &1_u64);
        range.add_param("max".into(), &99_u64);
        let mut character = ValidationErrors::new();
        character.add("level", range);

        let mut dynamic = ValidationError::new("custom");
        dynamic.code = std::borrow::Cow::Owned(format!("custom.{}", "quota"));
        dynamic.message = Some("Quota exceeded.".into());
        dynamic.add_param("value".into(), &12_u64);
        dynamic.add_param("min".into(), &1_u64);
        dynamic.add_param("max".into(), &10_u64);
        let mut item = ValidationErrors::new();
        item.add("quantity", dynamic);
        item.add("quantity", ValidationError::new("length"));

        let mut errors = ValidationErrors::new();
        errors.errors_mut().insert(
            "character",
            ValidationErrorsKind::Struct(Box::new(character)),
        );
        errors.errors_mut().insert(
            "items",
            ValidationErrorsKind::List(std::collections::BTreeMap::from([(2, Box::new(item))])),
        );
        let response = ValidationErrorResponse::from(errors);
        assert_eq!(
            serde_json::to_value(response).unwrap(),
            serde_json::json!({
                "status": "validation_error",
                "code": 422,
                "message": "The given data was invalid.",
                "errors": {
                    "character.level": [{
                        "code": "range",
                        "message": "Invalid field value: range",
                        "hint": "Ensure the value falls within the allowed minimum and maximum range.",
                        "rejected_value": 0,
                        "constraint": {"min": 1, "max": 99}
                    }],
                    "items[2].quantity": [{
                        "code": "custom.quota",
                        "message": "Quota exceeded.",
                        "hint": "Ensure the value satisfies the 'custom.quota' rule.",
                        "rejected_value": 12,
                        "constraint": {"min": 1, "max": 10}
                    }, {
                        "code": "length",
                        "message": "Invalid field value: length",
                        "hint": "Ensure the character length satisfies the minimum and maximum boundaries."
                    }]
                }
            })
        );
    }

    #[test]
    fn violation_code_schema_is_a_required_string() {
        let schema = serde_json::to_value(schemars::schema_for!(FieldViolation)).unwrap();
        assert_eq!(schema["properties"]["code"]["type"], "string");
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("code"))
        );
    }

    #[test]
    fn test_validation_response_serialization() {
        let mut resp = ValidationErrorResponse::new();
        resp.add_violation(
            "strength",
            FieldViolation::new(
                "overflow",
                "Strength exceeds allowable maximum value.",
                "Must result in a value between 1 and 1023. Current: 1000, Attempted delta: +50 (Result: 1050)",
            )
            .with_rejected_value(50)
            .with_constraint(1, 1023),
        );

        let json = serde_json::to_string_pretty(&resp).unwrap();
        assert!(json.contains("\"code\": 422"));
        assert!(json.contains("\"status\": \"validation_error\""));
        assert!(json.contains("\"strength\""));
        assert!(json.contains("\"overflow\""));
        assert!(json.contains("\"min\": 1"));
        assert!(json.contains("\"max\": 1023"));
    }
}
