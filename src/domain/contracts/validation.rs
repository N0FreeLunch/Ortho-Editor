//! Standardized 422 Unprocessable Entity validation response models.
//! Follows RFC 7807 / Laravel-style field-indexed violation arrays with actionable user guidance.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// Numeric or value constraints applicable to the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldViolation {
    /// Machine-readable violation code (e.g. "overflow", "underflow", "length", "invalid_range").
    pub code: &'static str,

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
    pub fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            code,
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

    pub fn with_constraint(mut self, min: impl Into<Option<u64>>, max: impl Into<Option<u64>>) -> Self {
        self.constraint = Some(ConstraintRange::new(min, max));
        self
    }
}

/// Standardized 422 Unprocessable Entity response payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
                        let code_str = Box::leak(fe.code.to_string().into_boxed_str());
                        let message = fe.message.map(|m| m.to_string()).unwrap_or_else(|| format!("Invalid field value: {}", fe.code));
                        let hint = match fe.code.as_ref() {
                            "length" => "Ensure the character length satisfies the minimum and maximum boundaries.".to_string(),
                            "range" => "Ensure the value falls within the allowed minimum and maximum range.".to_string(),
                            other => format!("Ensure the value satisfies the '{}' rule.", other),
                        };

                        let mut violation = FieldViolation::new(code_str, message, hint);
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
