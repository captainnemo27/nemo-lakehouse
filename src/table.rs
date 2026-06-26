use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::error::{NemoError, Result};
use crate::graph::{DimensionPredicate, QueryPlan};
use crate::metadata::{CompactionGroup, CompactionPlan, DataFile, QueryHistoryEntry, Snapshot, TableMetadata, VirtualFile};
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

    pub fn metadata_checksum_path(&self) -> PathBuf {
        checksum_path(&self.metadata_path())
    }

    pub fn snapshots_dir(&self) -> PathBuf {
        self.path.join("_nemo").join("snapshots")
    }

    pub fn snapshot_path(&self, snapshot_id: u64) -> PathBuf {
        self.snapshots_dir().join(format!("{snapshot_id:020}.json"))
    }

    pub fn snapshot_checksum_path(&self, snapshot_id: u64) -> PathBuf {
        checksum_path(&self.snapshot_path(snapshot_id))
    }

    pub fn load_metadata(&self) -> Result<TableMetadata> {
        let path = self.metadata_path();
        if !path.exists() {
            return Err(NemoError::TableNotFound(path));
        }
        verify_checksum_if_present(&path)?;
        let file = File::open(path)?;
        let metadata: TableMetadata = serde_json::from_reader(file)?;
        metadata.validate()?;
        Ok(metadata)
    }

    pub fn load_snapshot(&self, snapshot_id: u64) -> Result<Snapshot> {
        let path = self.snapshot_path(snapshot_id);
        verify_checksum_if_present(&path)?;
        let file = File::open(path)?;
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
        // Validate against domain rules if present in parent directory
        if let Some(parent) = self.path.parent() {
            let domain_meta_file = parent.join("_nemo_domain").join("domain.json");
            if domain_meta_file.exists() {
                let reader = std::fs::File::open(&domain_meta_file)?;
                let domain_meta: crate::domain::DomainMetadata = serde_json::from_reader(reader)?;
                for file in &data_files {
                    domain_meta.validate_data_file(file)?;
                }
            }
        }

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
            vec![],
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


    pub fn resolve_active_virtual_files(
        &self,
        target_snapshot_id: Option<u64>,
    ) -> Result<(HashSet<String>, BTreeMap<String, VirtualFile>)> {
        let metadata = self.load_metadata()?;
        self.resolve_active_virtual_files_with_metadata(&metadata, target_snapshot_id)
    }

    pub fn resolve_active_virtual_files_with_metadata(
        &self,
        metadata: &TableMetadata,
        target_snapshot_id: Option<u64>,
    ) -> Result<(HashSet<String>, BTreeMap<String, VirtualFile>)> {
        let start_id = match target_snapshot_id {
            Some(id) => id,
            None => match metadata.current_snapshot_id {
                Some(id) => id,
                None => return Ok((HashSet::new(), BTreeMap::new())),
            },
        };

        let mut active_ids = HashSet::new();
        let mut removed_ids = HashSet::new();
        let mut current_id = Some(start_id);

        while let Some(id) = current_id {
            let snapshot = self.load_snapshot(id)?;
            for vf_id in &snapshot.virtual_file_ids {
                if !removed_ids.contains(vf_id) {
                    active_ids.insert(vf_id.clone());
                }
            }
            for vf_id in &snapshot.removed_virtual_file_ids {
                removed_ids.insert(vf_id.clone());
            }
            current_id = snapshot.parent_snapshot_id;
        }

        let mut active_vfs = BTreeMap::new();
        for id in &active_ids {
            if let Some(vf) = metadata.virtual_files.get(id) {
                active_vfs.insert(id.clone(), vf.clone());
            } else {
                return Err(NemoError::Metadata(format!(
                    "Active virtual file {} is missing from table metadata virtual_files",
                    id
                )));
            }
        }

        Ok((active_ids, active_vfs))
    }

    pub fn resolve_active_virtual_files_with_data_files(
        &self,
        target_snapshot_id: Option<u64>,
    ) -> Result<Vec<(VirtualFile, Vec<DataFile>)>> {
        let metadata = self.load_metadata()?;
        self.resolve_active_virtual_files_with_data_files_with_metadata(&metadata, target_snapshot_id)
    }

    pub fn resolve_active_virtual_files_with_data_files_with_metadata(
        &self,
        metadata: &TableMetadata,
        target_snapshot_id: Option<u64>,
    ) -> Result<Vec<(VirtualFile, Vec<DataFile>)>> {
        let (active_ids, active_vfs) = self.resolve_active_virtual_files_with_metadata(metadata, target_snapshot_id)?;

        let mut result = Vec::new();
        let mut current_id = match target_snapshot_id {
            Some(id) => Some(id),
            None => metadata.current_snapshot_id,
        };

        let mut remaining_active_ids = active_ids.clone();
        while let Some(id) = current_id {
            if remaining_active_ids.is_empty() {
                break;
            }
            let snapshot = self.load_snapshot(id)?;
            for vf_id in &snapshot.virtual_file_ids {
                if remaining_active_ids.remove(vf_id) {
                    if let Some(vf) = active_vfs.get(vf_id) {
                        result.push((vf.clone(), snapshot.data_files.clone()));
                    }
                }
            }
            current_id = snapshot.parent_snapshot_id;
        }

        Ok(result)
    }

    pub fn plan_files(&self, predicates: BTreeMap<String, String>) -> Result<QueryPlan> {
        self.plan_files_at_snapshot(predicates, None)
    }

    pub fn plan_files_at_snapshot(
        &self,
        predicates: BTreeMap<String, String>,
        snapshot_id: Option<u64>,
    ) -> Result<QueryPlan> {
        let metadata = self.load_metadata()?;
        let (active_ids, active_vfs) = self.resolve_active_virtual_files(snapshot_id)?;
        let mut plan = metadata.graph.plan(&predicates, &active_vfs, &active_ids)?;
        self.populate_plan_delete_bitmaps(&mut plan, snapshot_id)?;
        self.record_query_entry(QueryHistoryEntry {
            created_at: Utc::now(),
            dimensions: sorted_keys(predicates.keys().cloned()),
            equality_predicates: predicates,
            range_predicates: BTreeMap::new(),
        })?;
        Ok(plan)
    }

    pub fn plan_files_with_predicates(
        &self,
        predicates: BTreeMap<String, DimensionPredicate>,
    ) -> Result<QueryPlan> {
        self.plan_files_with_predicates_at_snapshot(predicates, None)
    }

    pub fn plan_files_with_predicates_at_snapshot(
        &self,
        predicates: BTreeMap<String, DimensionPredicate>,
        snapshot_id: Option<u64>,
    ) -> Result<QueryPlan> {
        let metadata = self.load_metadata()?;
        let (active_ids, active_vfs) = self.resolve_active_virtual_files(snapshot_id)?;
        let mut plan = metadata
            .graph
            .plan_with_predicates(&predicates, &active_vfs, &active_ids)?;
        self.populate_plan_delete_bitmaps(&mut plan, snapshot_id)?;
        self.record_query_entry(history_entry_from_predicates(predicates))?;
        Ok(plan)
    }

    pub fn compact_files(
        &self,
        partition: BTreeMap<String, String>,
        target_file_name: String,
    ) -> Result<Snapshot> {
        let mut metadata = self.load_metadata()?;

        for dim in &metadata.graph.dimensions {
            if !partition.contains_key(dim) {
                return Err(NemoError::Commit(format!(
                    "compaction requires fully specified leaf partition: missing dimension {}",
                    dim
                )));
            }
        }

        let (_active_ids, active_vfs) = self.resolve_active_virtual_files_with_metadata(&metadata, None)?;
        let active_ids = active_vfs.keys().cloned().collect();
        let plan = metadata.graph.plan(&partition, &active_vfs, &active_ids)?;
        let vf_ids_to_remove: Vec<String> = plan.virtual_files.iter().map(|vf| vf.id.clone()).collect();
        let total_physical_files: usize = plan.virtual_files.iter().map(|vf| vf.physical_files.len()).sum();
        let total_records: u64 = plan.virtual_files.iter().map(|vf| vf.record_count).sum();

        if total_physical_files <= 1 {
            return Err(NemoError::Commit("nothing to compact: partition has 0 or 1 physical files".into()));
        }

        let compacted_data_file = DataFile::new(
            target_file_name,
            total_records,
            partition.into_iter().collect(),
        )?;

        let parent_snapshot_id = metadata.current_snapshot_id;
        let snapshot_id = parent_snapshot_id.unwrap_or(0) + 1;
        let target_vf_id = format!("vf-{snapshot_id:020}");

        let compacted_vf = VirtualFile::from_data_files(&target_vf_id, &[compacted_data_file.clone()])?;

        let snapshot = Snapshot::new(
            snapshot_id,
            parent_snapshot_id,
            vec![compacted_data_file.clone()],
            vec![target_vf_id.clone()],
            vf_ids_to_remove.clone(),
        )?;

        let snapshot_path = self.snapshot_path(snapshot_id);
        if snapshot_path.exists() {
            return Err(NemoError::Commit(format!(
                "snapshot already exists: {}",
                snapshot_path.display()
            )));
        }

        metadata.graph.insert_virtual_file(&compacted_vf, &[compacted_data_file])?;
        metadata.virtual_files.insert(target_vf_id, compacted_vf);
        metadata.current_snapshot_id = Some(snapshot_id);
        metadata.updated_at = Utc::now();

        atomic_write_json(&snapshot_path, &snapshot)?;
        atomic_write_json(&self.metadata_path(), &metadata)?;

        Ok(snapshot)
    }

    pub fn compact_plan(
        &self,
        partition: BTreeMap<String, String>,
        target_file_name: Option<String>,
    ) -> Result<CompactionPlan> {
        let metadata = self.load_metadata()?;
        for dim in &metadata.graph.dimensions {
            if !partition.contains_key(dim) {
                return Err(NemoError::Commit(format!(
                    "compaction planning requires fully specified leaf partition: missing dimension {}",
                    dim
                )));
            }
        }

        let (_active_ids, active_vfs) = self.resolve_active_virtual_files_with_metadata(&metadata, None)?;
        let active_ids = active_vfs.keys().cloned().collect();
        let plan = metadata.graph.plan(&partition, &active_vfs, &active_ids)?;
        let source_virtual_file_ids: Vec<String> = plan.virtual_files.iter().map(|vf| vf.id.clone()).collect();
        let physical_files: Vec<String> = plan
            .virtual_files
            .iter()
            .flat_map(|vf| vf.physical_files.clone())
            .collect();
        let record_count: u64 = plan.virtual_files.iter().map(|vf| vf.record_count).sum();

        let groups = if physical_files.len() > 1 {
            vec![CompactionGroup {
                source_virtual_file_ids,
                physical_files,
                record_count,
                suggested_virtual_file_id: format!(
                    "vf-plan-{:020}",
                    metadata.current_snapshot_id.unwrap_or(0) + 1
                ),
                suggested_physical_file: target_file_name.unwrap_or_else(|| {
                    let suffix = metadata.current_snapshot_id.unwrap_or(0) + 1;
                    format!("data/compact-{suffix:020}.parquet")
                }),
            }]
        } else {
            Vec::new()
        };

        Ok(CompactionPlan { partition, groups })
    }

    pub fn delete_rows(
        &self,
        physical_file_path: &str,
        delete_bitmap_ref: String,
    ) -> Result<Snapshot> {
        let mut metadata = self.load_metadata()?;
        let (_active_ids, active_vfs) = self.resolve_active_virtual_files_with_metadata(&metadata, None)?;

        let matching_vf_entry = active_vfs
            .iter()
            .find(|(_, vf)| vf.physical_files.iter().any(|p| p == physical_file_path));

        let (old_vf_id, old_vf) = match matching_vf_entry {
            Some((id, vf)) => (id.clone(), vf.clone()),
            None => {
                return Err(NemoError::Commit(format!(
                    "physical file {} is not active in the current snapshot",
                    physical_file_path
                )))
            }
        };

        let mut old_data_file = None;
        let mut current_id = metadata.current_snapshot_id;
        while let Some(id) = current_id {
            let snapshot = self.load_snapshot(id)?;
            if let Some(df) = snapshot.data_files.iter().find(|df| df.path == physical_file_path) {
                old_data_file = Some(df.clone());
                break;
            }
            current_id = snapshot.parent_snapshot_id;
        }

        let old_data_file = match old_data_file {
            Some(df) => df,
            None => {
                return Err(NemoError::Commit(format!(
                    "physical file {} could not be found in active lineage",
                    physical_file_path
                )))
            }
        };

        let mut updated_data_file = old_data_file.clone();
        updated_data_file.column_stats.insert(
            "_delete_bitmap".to_string(),
            crate::metadata::ColumnStats {
                min: None,
                max: None,
                null_count: 0,
                ndv: None,
                bloom_ref: None,
                delete_bitmap_ref: Some(delete_bitmap_ref),
            },
        );

        let parent_snapshot_id = metadata.current_snapshot_id;
        let snapshot_id = parent_snapshot_id.unwrap_or(0) + 1;
        let new_vf_id = format!("vf-{snapshot_id:020}");

        let new_physical_files = old_vf.physical_files.clone();
        let new_vf = VirtualFile {
            id: new_vf_id.clone(),
            physical_files: new_physical_files,
            record_count: old_vf.record_count,
        };

        let snapshot = Snapshot::new(
            snapshot_id,
            parent_snapshot_id,
            vec![updated_data_file.clone()],
            vec![new_vf_id.clone()],
            vec![old_vf_id.clone()],
        )?;

        let snapshot_path = self.snapshot_path(snapshot_id);
        if snapshot_path.exists() {
            return Err(NemoError::Commit(format!(
                "snapshot already exists: {}",
                snapshot_path.display()
            )));
        }

        metadata.graph.insert_virtual_file(&new_vf, &[updated_data_file])?;
        metadata.virtual_files.insert(new_vf_id, new_vf);
        metadata.current_snapshot_id = Some(snapshot_id);
        metadata.updated_at = Utc::now();

        atomic_write_json(&snapshot_path, &snapshot)?;
        atomic_write_json(&self.metadata_path(), &metadata)?;

        Ok(snapshot)
    }

    pub fn record_query(&self, dimensions: &[String]) -> Result<()> {
        self.record_query_entry(QueryHistoryEntry {
            created_at: Utc::now(),
            dimensions: dimensions.to_vec(),
            equality_predicates: BTreeMap::new(),
            range_predicates: BTreeMap::new(),
        })
    }

    pub fn optimize_layout(&self, dry_run: bool) -> Result<Option<Vec<String>>> {
        let mut metadata = self.load_metadata()?;

        let history_path = self.path.join("_nemo").join("query_history.json");
        if !history_path.exists() {
            return Ok(None);
        }

        let history = self.load_query_history()?;
        if history.is_empty() {
            return Ok(None);
        }

        // Count frequency of queried dimensions
        let mut frequency_map = std::collections::HashMap::new();
        for query in history {
            for dim in query.dimensions {
                *frequency_map.entry(dim).or_insert(0) += 1;
            }
        }

        // Stable sort current graph dimensions by query frequency descending
        let mut new_dimensions = metadata.graph.dimensions.clone();
        new_dimensions.sort_by_key(|dim| {
            let freq = frequency_map.get(dim).cloned().unwrap_or(0);
            std::cmp::Reverse(freq)
        });

        // If order hasn't changed, return None
        if new_dimensions == metadata.graph.dimensions {
            return Ok(None);
        }

        if !dry_run {
            self.rebuild_graph(&mut metadata, new_dimensions.clone())?;
            metadata.updated_at = Utc::now();
            atomic_write_json(&self.metadata_path(), &metadata)?;
        }

        Ok(Some(new_dimensions))
    }

    pub fn validate_integrity(&self) -> Result<()> {
        verify_checksum_required(&self.metadata_path())?;
        let metadata = self.load_metadata()?;

        let mut current_id = metadata.current_snapshot_id;
        let mut seen_snapshots = HashSet::new();
        while let Some(snapshot_id) = current_id {
            if !seen_snapshots.insert(snapshot_id) {
                return Err(NemoError::Integrity(format!(
                    "snapshot lineage cycle detected at snapshot {}",
                    snapshot_id
                )));
            }
            verify_checksum_required(&self.snapshot_path(snapshot_id))?;
            let snapshot = self.load_snapshot(snapshot_id)?;
            for vf_id in snapshot
                .virtual_file_ids
                .iter()
                .chain(snapshot.removed_virtual_file_ids.iter())
            {
                if !metadata.virtual_files.contains_key(vf_id) && snapshot.virtual_file_ids.contains(vf_id) {
                    return Err(NemoError::Integrity(format!(
                        "snapshot {} references missing virtual file {}",
                        snapshot_id, vf_id
                    )));
                }
            }
            current_id = snapshot.parent_snapshot_id;
        }

        let (active_ids, active_vfs) = self.resolve_active_virtual_files_with_metadata(&metadata, None)?;
        let plan = metadata.graph.plan_with_predicates(&BTreeMap::new(), &active_vfs, &active_ids)?;
        for virtual_file in &plan.virtual_files {
            if !metadata.virtual_files.contains_key(&virtual_file.id) {
                return Err(NemoError::Integrity(format!(
                    "graph references missing virtual file {}",
                    virtual_file.id
                )));
            }
        }
        Ok(())
    }

    pub fn query_history(&self) -> Result<Vec<QueryHistoryEntry>> {
        self.load_query_history()
    }

    pub fn rebuild_graph(&self, metadata: &mut TableMetadata, dimensions: Vec<String>) -> Result<()> {
        let mut new_graph = crate::graph::MetadataGraph::new(dimensions)?;
        let snapshots = self.snapshot_history()?;
        for snapshot in snapshots {
            for vf_id in &snapshot.virtual_file_ids {
                if let Some(vf) = metadata.virtual_files.get(vf_id) {
                    let matching_data_files: Vec<DataFile> = snapshot.data_files
                        .iter()
                        .filter(|df| vf.physical_files.contains(&df.path))
                        .cloned()
                        .collect();
                    if !matching_data_files.is_empty() {
                        new_graph.insert_virtual_file(vf, &matching_data_files)?;
                    }
                }
            }
        }
        metadata.graph = new_graph;
        Ok(())
    }

    fn populate_plan_delete_bitmaps(&self, plan: &mut QueryPlan, snapshot_id: Option<u64>) -> Result<()> {
        let metadata = self.load_metadata()?;
        let start_id = match snapshot_id {
            Some(id) => id,
            None => match metadata.current_snapshot_id {
                Some(id) => id,
                None => return Ok(()),
            },
        };

        let physical_paths: HashSet<String> = plan
            .virtual_files
            .iter()
            .flat_map(|vf| vf.physical_files.clone())
            .collect();

        let mut current_id = Some(start_id);
        while let Some(id) = current_id {
            let snapshot = self.load_snapshot(id)?;
            for data_file in &snapshot.data_files {
                if physical_paths.contains(&data_file.path) {
                    for stats in data_file.column_stats.values() {
                        if let Some(bitmap_ref) = &stats.delete_bitmap_ref {
                            plan.delete_bitmaps
                                .insert(data_file.path.clone(), bitmap_ref.clone());
                        }
                    }
                }
            }
            current_id = snapshot.parent_snapshot_id;
        }
        Ok(())
    }

    fn record_query_entry(&self, entry: QueryHistoryEntry) -> Result<()> {
        let history_path = self.path.join("_nemo").join("query_history.json");
        let mut history = self.load_query_history()?;
        history.push(entry);
        atomic_write_json(&history_path, &history)?;
        Ok(())
    }

    fn load_query_history(&self) -> Result<Vec<QueryHistoryEntry>> {
        let history_path = self.path.join("_nemo").join("query_history.json");
        if !history_path.exists() {
            return Ok(Vec::new());
        }
        let file = File::open(&history_path)?;
        let value: serde_json::Value = serde_json::from_reader(file)?;
        if value.as_array().is_some_and(|items| {
            items
                .first()
                .is_some_and(|item| item.as_array().is_some())
        }) {
            let legacy: Vec<Vec<String>> = serde_json::from_value(value)?;
            return Ok(legacy
                .into_iter()
                .map(|dimensions| QueryHistoryEntry {
                    created_at: Utc::now(),
                    dimensions,
                    equality_predicates: BTreeMap::new(),
                    range_predicates: BTreeMap::new(),
                })
                .collect());
        }
        Ok(serde_json::from_value(value).unwrap_or_default())
    }
}

fn checksum_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.sha256",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("file")
    ))
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn verify_checksum_if_present(path: &Path) -> Result<()> {
    let checksum = checksum_path(path);
    if checksum.exists() {
        verify_checksum_required(path)?;
    }
    Ok(())
}

fn verify_checksum_required(path: &Path) -> Result<()> {
    let checksum = checksum_path(path);
    if !checksum.exists() {
        return Err(NemoError::Integrity(format!(
            "checksum sidecar missing for {}",
            path.display()
        )));
    }
    let expected = fs::read_to_string(&checksum)?.trim().to_string();
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    let actual = sha256_hex(&bytes);
    if actual != expected {
        return Err(NemoError::Integrity(format!(
            "checksum mismatch for {}",
            path.display()
        )));
    }
    Ok(())
}

fn atomic_write_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| NemoError::InvalidPath(format!("path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)?;
    let tmp_path = path.with_file_name(format!(
        ".{}.tmp",
        path.file_name().and_then(|name| name.to_str()).unwrap_or("file")
    ));
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(tmp_path, path)?;
    Ok(())
}

fn atomic_write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    atomic_write_bytes(path, &bytes)?;
    let checksum = format!("{}\n", sha256_hex(&bytes));
    atomic_write_bytes(&checksum_path(path), checksum.as_bytes())?;
    Ok(())
}

fn sorted_keys(keys: impl Iterator<Item = String>) -> Vec<String> {
    let mut keys: Vec<String> = keys.collect();
    keys.sort();
    keys.dedup();
    keys
}

fn history_entry_from_predicates(predicates: BTreeMap<String, DimensionPredicate>) -> QueryHistoryEntry {
    let dimensions = sorted_keys(predicates.keys().cloned());
    let mut equality_predicates = BTreeMap::new();
    let mut range_predicates = BTreeMap::new();
    for (dimension, predicate) in predicates {
        match predicate {
            DimensionPredicate::Equal(value) => {
                equality_predicates.insert(dimension, value);
            }
            DimensionPredicate::Range { lower, upper, .. } => {
                range_predicates.insert(dimension, (lower, upper));
            }
        }
    }
    QueryHistoryEntry {
        created_at: Utc::now(),
        dimensions,
        equality_predicates,
        range_predicates,
    }
}
