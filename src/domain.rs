use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::error::{NemoError, Result};
use crate::metadata::DataFile;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainMetadata {
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<DomainRule>,
    pub relations: Vec<Relation>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DomainRule {
    pub column_name: String,
    pub constraint: Constraint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "config")]
pub enum Constraint {
    NotNull,
    MinMax { min: Option<String>, max: Option<String> },
    AllowedValues(Vec<String>),
    RegexMatch(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Relation {
    pub from_table: String,
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

impl DomainMetadata {
    pub fn new(
        name: String,
        description: Option<String>,
        rules: Vec<DomainRule>,
        relations: Vec<Relation>,
    ) -> Self {
        let now = Utc::now();
        Self {
            name,
            description,
            rules,
            relations,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate_data_file(&self, file: &DataFile) -> Result<()> {
        for rule in &self.rules {
            let col = &rule.column_name;
            // 1. Check partition value if it is a partition column
            let val_from_part = file.partition_values.get(col).cloned();
            
            // 2. Check stats if it is in column_stats
            let stats = file.column_stats.get(col);

            match &rule.constraint {
                Constraint::NotNull => {
                    if let Some(val) = &val_from_part {
                        if val.trim().is_empty() {
                            return Err(NemoError::Validation(format!(
                                "Domain rule violation (NotNull): partition column '{}' is empty",
                                col
                            )));
                        }
                    }
                    if let Some(st) = stats {
                        if st.null_count > 0 {
                            return Err(NemoError::Validation(format!(
                                "Domain rule violation (NotNull): column '{}' has null count {}",
                                col, st.null_count
                            )));
                        }
                    }
                }
                Constraint::MinMax { min, max } => {
                    let check_bounds = |val: &str| -> Result<()> {
                        if let Some(min_val) = min {
                            if !compare_min_max(val, min_val, true) {
                                return Err(NemoError::Validation(format!(
                                    "Domain rule violation (MinMax): value '{}' in column '{}' is below minimum '{}'",
                                    val, col, min_val
                                )));
                            }
                        }
                        if let Some(max_val) = max {
                            if !compare_min_max(val, max_val, false) {
                                return Err(NemoError::Validation(format!(
                                    "Domain rule violation (MinMax): value '{}' in column '{}' is above maximum '{}'",
                                    val, col, max_val
                                )));
                            }
                        }
                        Ok(())
                    };

                    if let Some(val) = &val_from_part {
                        check_bounds(val)?;
                    }

                    if let Some(st) = stats {
                        if let Some(st_min) = &st.min {
                            check_bounds(st_min)?;
                        }
                        if let Some(st_max) = &st.max {
                            check_bounds(st_max)?;
                        }
                    }
                }
                Constraint::AllowedValues(allowed) => {
                    if let Some(val) = &val_from_part {
                        if !allowed.contains(val) {
                            return Err(NemoError::Validation(format!(
                                "Domain rule violation (AllowedValues): value '{}' in partition column '{}' is not in allowed list {:?}",
                                val, col, allowed
                            )));
                        }
                    }
                    if let Some(st) = stats {
                        if let Some(st_min) = &st.min {
                            if !allowed.contains(st_min) {
                                return Err(NemoError::Validation(format!(
                                    "Domain rule violation (AllowedValues): min value '{}' in column '{}' is not in allowed list {:?}",
                                    st_min, col, allowed
                                )));
                            }
                        }
                        if let Some(st_max) = &st.max {
                            if !allowed.contains(st_max) {
                                return Err(NemoError::Validation(format!(
                                    "Domain rule violation (AllowedValues): max value '{}' in column '{}' is not in allowed list {:?}",
                                    st_max, col, allowed
                                )));
                            }
                        }
                    }
                }
                Constraint::RegexMatch(pattern) => {
                    let re = regex::Regex::new(pattern).map_err(|e| {
                        NemoError::Validation(format!(
                            "Invalid regex pattern '{}' in domain rule for column '{}': {}",
                            pattern, col, e
                        ))
                    })?;

                    if let Some(val) = &val_from_part {
                        if !re.is_match(val) {
                            return Err(NemoError::Validation(format!(
                                "Domain rule violation (RegexMatch): partition column '{}' value '{}' does not match pattern '{}'",
                                col, val, pattern
                            )));
                        }
                    }
                    if let Some(st) = stats {
                        if let Some(st_min) = &st.min {
                            if !re.is_match(st_min) {
                                return Err(NemoError::Validation(format!(
                                    "Domain rule violation (RegexMatch): column '{}' min value '{}' does not match pattern '{}'",
                                    col, st_min, pattern
                                )));
                            }
                        }
                        if let Some(st_max) = &st.max {
                            if !re.is_match(st_max) {
                                return Err(NemoError::Validation(format!(
                                    "Domain rule violation (RegexMatch): column '{}' max value '{}' does not match pattern '{}'",
                                    col, st_max, pattern
                                )));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn compare_min_max(val: &str, limit: &str, is_min: bool) -> bool {
    if let (Ok(v_num), Ok(l_num)) = (val.parse::<f64>(), limit.parse::<f64>()) {
        if is_min {
            v_num >= l_num
        } else {
            v_num <= l_num
        }
    } else {
        if is_min {
            val >= limit
        } else {
            val <= limit
        }
    }
}
