use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{NemoError, Result};
use crate::schema::Schema;
use crate::table::Table;

#[derive(Debug, Clone)]
pub struct LocalCatalog {
    warehouse: PathBuf,
}

impl LocalCatalog {
    pub fn new(warehouse: impl Into<PathBuf>) -> Self {
        Self {
            warehouse: warehouse.into(),
        }
    }

    pub fn table_path(&self, table_name: &str) -> Result<PathBuf> {
        validate_table_name(table_name)?;
        Ok(self.warehouse.join(table_name.replace('.', "/")))
    }

    pub fn create_table(&self, table_name: &str, schema: Schema, graph_dimensions: Vec<String>) -> Result<Table> {
        Table::create(self.table_path(table_name)?, table_name, schema, graph_dimensions)
    }

    pub fn load_table(&self, table_name: &str) -> Result<Table> {
        Ok(Table::new(self.table_path(table_name)?))
    }

    pub fn list_tables(&self) -> Result<Vec<String>> {
        let mut tables = Vec::new();
        if !self.warehouse.exists() {
            return Ok(tables);
        }
        collect_tables(&self.warehouse, &self.warehouse, &mut tables)?;
        tables.sort();
        Ok(tables)
    }
}

fn collect_tables(root: &Path, current: &Path, output: &mut Vec<String>) -> Result<()> {
    if current.join("_nemo").join("metadata.json").exists() {
        let relative = current.strip_prefix(root).map_err(|error| NemoError::Metadata(error.to_string()))?;
        output.push(
            relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("."),
        );
        return Ok(());
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            collect_tables(root, &entry.path(), output)?;
        }
    }
    Ok(())
}

fn validate_table_name(value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(NemoError::InvalidPath("table name must not be empty".into()));
    }
    if value.starts_with('.')
        || value.ends_with('.')
        || value.contains("..")
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(NemoError::InvalidPath(format!("invalid table name: {value:?}")));
    }
    Ok(())
}

