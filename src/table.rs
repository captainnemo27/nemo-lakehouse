use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::Utc;

use crate::error::{NemoError, Result};
use crate::graph::QueryPlan;
use crate::metadata::{DataFile, Snapshot, TableMetadata, VirtualFile};
use crate::schema::Schema;

#[derive(Debug, Clone)]
pub struct Table {
    path: PathBuf,
}

impl Table {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn create(
        path: impl Into<PathBuf>,
        table_name: impl Into<String>,
        schema: Schema,
        graph_dimensions: Vec<String>,
    ) -> Result<Self> {
        let table = Self::new(path);
        if table.metadata_path().exists() {
            return Err(NemoError::TableAlreadyExists(table.path));
        }
        fs::create_dir_all(table.snapshots_dir())?;
        let metadata = TableMetadata::new(table_name, schema, graph_dimensions)?;
        atomic_write_json(&table.metadata_path(), &metadata)?;
        Ok(table)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn metadata_path(&self) -> PathBuf {
        self.path.join("_nemo").join("metadata.json")
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.path.join("_nemo").join("snapshots")
    }

    pub fn snapshot_path(&self, snapshot_id: u64) -> PathBuf {
        self.snapshots_dir().join(format!("{snapshot_id:020}.json"))
    }

    pub fn load_metadata(&self) -> Result<TableMetadata> {
        let path = self.metadata_path();
        if !path.exists() {
            return Err(NemoError::TableNotFound(path));
        }
        let file = File::open(path)?;
        let metadata: TableMetadata = serde_json::from_reader(file)?;
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn load_snapshot(&self, snapshot_id: u64) -> Result<Snapshot> {
        let file = File::open(self.snapshot_path(snapshot_id))?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn snapshot_history(&self) -> Result<Vec<Snapshot>> {
        let mut snapshots = Vec::new();
        if !self.snapshots_dir().exists() {
            return Ok(snapshots);
        }
        for entry in fs::read_dir(self.snapshots_dir())? {
            let path = entry?.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                snapshots.push(serde_json::from_reader(File::open(path)?)?);
            }
        }
        snapshots.sort_by_key(|snapshot: &Snapshot| snapshot.snapshot_id);
        Ok(snapshots)
    }

    pub fn append_files(&self, data_files: Vec<DataFile>) -> Result<Snapshot> {
        let mut metadata = self.load_metadata()?;
        let parent_snapshot_id = metadata.current_snapshot_id;
        let snapshot_id = parent_snapshot_id.unwrap_or(0) + 1;
        let virtual_file_id = format!("vf-{snapshot_id:020}");
        let virtual_file = VirtualFile::from_data_files(&virtual_file_id, &data_files)?;
        let snapshot = Snapshot::new(
            snapshot_id,
            parent_snapshot_id,
            data_files.clone(),
            vec![virtual_file_id.clone()],
        )?;

        let snapshot_path = self.snapshot_path(snapshot_id);
        if snapshot_path.exists() {
            return Err(NemoError::Commit(format!(
                "snapshot already exists: {}",
                snapshot_path.display()
            )));
        }

        metadata.graph.insert_virtual_file(&virtual_file, &data_files)?;
        metadata.virtual_files.insert(virtual_file_id, virtual_file);
        metadata.current_snapshot_id = Some(snapshot_id);
        metadata.updated_at = Utc::now();

        atomic_write_json(&snapshot_path, &snapshot)?;
        atomic_write_json(&self.metadata_path(), &metadata)?;
        Ok(snapshot)
    }

    pub fn plan_files(&self, predicates: BTreeMap<String, String>) -> Result<QueryPlan> {
        let metadata = self.load_metadata()?;
        Ok(metadata.graph.plan(&predicates, &metadata.virtual_files))
    }
}

fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| NemoError::InvalidPath(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)?;
    let tmp_path = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("metadata")
    ));
    {
        let mut file = File::create(&tmp_path)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)?;
    Ok(())
}
