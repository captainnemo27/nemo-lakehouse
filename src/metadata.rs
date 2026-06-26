use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::{NemoError, Result};
use crate::graph::MetadataGraph;
use crate::schema::Schema;

pub const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataFile {
    pub path: String,
    pub record_count: u64,
    #[serde(default)]
    pub partition_values: BTreeMap<String, String>,
    #[serde(default)]
    pub column_stats: HashMap<String, ColumnStats>,
}

impl DataFile {
    pub fn new(path: impl Into<String>, record_count: u64, partition_values: BTreeMap<String, String>) -> Result<Self> {
        let data_file = Self {
            path: path.into(),
            record_count,
            partition_values,
            column_stats: HashMap::new(),
        };
        data_file.validate()?;
        Ok(data_file)
    }

    pub fn validate(&self) -> Result<()> {
        if self.path.trim().is_empty() {
            return Err(NemoError::Metadata("data file path is required".into()));
        }
        if self.path.starts_with('/') || self.path.split('/').any(|part| part == "..") {
            return Err(NemoError::InvalidPath(format!(
                "data file path must stay inside the table: {}",
                self.path
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnStats {
    pub min: Option<String>,
    pub max: Option<String>,
    pub null_count: u64,
    pub ndv: Option<u64>,
    pub bloom_ref: Option<String>,
    pub delete_bitmap_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VirtualFile {
    pub id: String,
    pub physical_files: Vec<String>,
    pub record_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionPlan {
    pub partition: BTreeMap<String, String>,
    pub groups: Vec<CompactionGroup>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionGroup {
    pub source_virtual_file_ids: Vec<String>,
    pub physical_files: Vec<String>,
    pub record_count: u64,
    pub suggested_virtual_file_id: String,
    pub suggested_physical_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueryHistoryEntry {
    pub created_at: DateTime<Utc>,
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub equality_predicates: BTreeMap<String, String>,
    #[serde(default)]
    pub range_predicates: BTreeMap<String, (Option<String>, Option<String>)>,
}

impl VirtualFile {
    pub fn from_data_files(id: impl Into<String>, data_files: &[DataFile]) -> Result<Self> {
        if data_files.is_empty() {
            return Err(NemoError::Commit("virtual file requires at least one physical file".into()));
        }
        Ok(Self {
            id: id.into(),
            physical_files: data_files.iter().map(|file| file.path.clone()).collect(),
            record_count: data_files.iter().map(|file| file.record_count).sum(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub snapshot_id: u64,
    pub parent_snapshot_id: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub data_files: Vec<DataFile>,
    pub virtual_file_ids: Vec<String>,
    #[serde(default)]
    pub removed_virtual_file_ids: Vec<String>,
}

impl Snapshot {
    pub fn new(
        snapshot_id: u64,
        parent_snapshot_id: Option<u64>,
        data_files: Vec<DataFile>,
        virtual_file_ids: Vec<String>,
        removed_virtual_file_ids: Vec<String>,
    ) -> Result<Self> {
        if data_files.is_empty() && virtual_file_ids.is_empty() && removed_virtual_file_ids.is_empty() {
            return Err(NemoError::Commit("snapshot requires at least one data file, virtual file, or removed virtual file".into()));
        }
        for data_file in &data_files {
            data_file.validate()?;
        }
        Ok(Self {
            snapshot_id,
            parent_snapshot_id,
            created_at: Utc::now(),
            data_files,
            virtual_file_ids,
            removed_virtual_file_ids,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableMetadata {
    pub table_name: String,
    pub format_version: u32,
    pub schema: Schema,
    pub current_snapshot_id: Option<u64>,
    pub graph: MetadataGraph,
    #[serde(default)]
    pub virtual_files: BTreeMap<String, VirtualFile>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl TableMetadata {
    pub fn new(table_name: impl Into<String>, schema: Schema, graph_dimensions: Vec<String>) -> Result<Self> {
        schema.validate()?;
        let table_name = table_name.into();
        if table_name.trim().is_empty() {
            return Err(NemoError::Metadata("table name is required".into()));
        }
        let now = Utc::now();
        Ok(Self {
            table_name,
            format_version: FORMAT_VERSION,
            schema,
            current_snapshot_id: None,
            graph: MetadataGraph::new(graph_dimensions)?,
            virtual_files: BTreeMap::new(),
            created_at: now,
            updated_at: now,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if self.format_version != FORMAT_VERSION {
            return Err(NemoError::Metadata(format!(
                "unsupported format version: {}",
                self.format_version
            )));
        }
        self.schema.validate()
    }

    pub fn rebuild_graph_with_dimensions(
        &mut self,
        dimensions: Vec<String>,
        active_vfs_with_data: &[(VirtualFile, Vec<DataFile>)],
    ) -> Result<()> {
        let mut new_graph = MetadataGraph::new(dimensions)?;
        for (vf, data_files) in active_vfs_with_data {
            new_graph.insert_virtual_file(vf, data_files)?;
        }
        self.graph = new_graph;
        Ok(())
    }
}
