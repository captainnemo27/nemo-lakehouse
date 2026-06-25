use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::{NemoError, Result};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    Boolean,
    Int,
    Long,
    Float,
    Double,
    String,
    Binary,
    Date,
    Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default)]
    pub required: bool,
}

impl Field {
    pub fn new(name: impl Into<String>, field_type: FieldType, required: bool) -> Self {
        Self {
            name: name.into(),
            field_type,
            required,
        }
    }

    pub fn validate(&self) -> Result<()> {
        validate_identifier(&self.name)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    pub fields: Vec<Field>,
}

impl Schema {
    pub fn new(fields: Vec<Field>) -> Result<Self> {
        let schema = Self { fields };
        schema.validate()?;
        Ok(schema)
    }

    pub fn validate(&self) -> Result<()> {
        if self.fields.is_empty() {
            return Err(NemoError::Schema("schema must contain at least one field".into()));
        }
        let mut names = HashSet::new();
        for field in &self.fields {
            field.validate()?;
            if !names.insert(field.name.as_str()) {
                return Err(NemoError::Schema(format!("duplicate field name: {}", field.name)));
            }
        }
        Ok(())
    }
}

pub fn validate_identifier(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(NemoError::Schema("identifier must not be empty".into()));
    }
    if value.contains('/') || value.contains('\\') || value.contains('\0') {
        return Err(NemoError::Schema(format!("invalid identifier: {value:?}")));
    }
    Ok(())
}

