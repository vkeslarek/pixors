use serde::{Deserialize, Serialize};

/// Serializable parameter value, used in mutations and preview overrides.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ParamValue {
    F32(f32),
    I32(i32),
    Bool(bool),
}

/// Describes one editable parameter of a filter, ready for generic UI rendering.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    pub name: &'static str,
    pub label: &'static str,
    pub kind: ParamKind,
}

#[derive(Debug, Clone)]
pub enum ParamKind {
    Float {
        value: f32,
        range: std::ops::RangeInclusive<f32>,
    },
    Int {
        value: i32,
        range: std::ops::RangeInclusive<i32>,
    },
    Bool {
        value: bool,
    },
}

impl ParamSpec {
    pub fn float(
        name: &'static str,
        label: &'static str,
        value: f32,
        range: std::ops::RangeInclusive<f32>,
    ) -> Self {
        Self {
            name,
            label,
            kind: ParamKind::Float { value, range },
        }
    }
    pub fn int(
        name: &'static str,
        label: &'static str,
        value: i32,
        range: std::ops::RangeInclusive<i32>,
    ) -> Self {
        Self {
            name,
            label,
            kind: ParamKind::Int { value, range },
        }
    }
    pub fn bool(name: &'static str, label: &'static str, value: bool) -> Self {
        Self {
            name,
            label,
            kind: ParamKind::Bool { value },
        }
    }
}
