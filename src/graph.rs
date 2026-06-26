use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::{NemoError, Result};
use crate::metadata::{DataFile, VirtualFile};
use crate::schema::validate_identifier;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DimensionPredicate {
    Equal(String),
    Range {
        lower: Option<String>,
        upper: Option<String>,
        lower_inclusive: bool,
        upper_inclusive: bool,
    },
}

impl DimensionPredicate {
    pub fn equal(value: impl Into<String>) -> Self {
        Self::Equal(value.into())
    }

    pub fn range(
        lower: Option<impl Into<String>>,
        upper: Option<impl Into<String>>,
        lower_inclusive: bool,
        upper_inclusive: bool,
    ) -> Self {
        Self::Range {
            lower: lower.map(Into::into),
            upper: upper.map(Into::into),
            lower_inclusive,
            upper_inclusive,
        }
    }

    pub fn inclusive_range(lower: impl Into<String>, upper: impl Into<String>) -> Self {
        Self::Range {
            lower: Some(lower.into()),
            upper: Some(upper.into()),
            lower_inclusive: true,
            upper_inclusive: true,
        }
    }

    fn matches(&self, value: &str) -> bool {
        match self {
            Self::Equal(expected) => value == expected,
            Self::Range {
                lower,
                upper,
                lower_inclusive,
                upper_inclusive,
            } => {
                let lower_matches = lower.as_deref().map_or(true, |bound| {
                    if *lower_inclusive {
                        value >= bound
                    } else {
                        value > bound
                    }
                });
                let upper_matches = upper.as_deref().map_or(true, |bound| {
                    if *upper_inclusive {
                        value <= bound
                    } else {
                        value < bound
                    }
                });
                lower_matches && upper_matches
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeStats {
    pub row_count: u64,
    pub physical_file_count: u64,
    pub virtual_file_count: u64,
}

impl NodeStats {
    fn add_file(&mut self, data_file: &DataFile) {
        self.row_count += data_file.record_count;
        self.physical_file_count += 1;
    }

    fn add_virtual_file(&mut self) {
        self.virtual_file_count += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphNode {
    pub value: String,
    #[serde(default)]
    pub stats: NodeStats,
    #[serde(default)]
    pub physical_files: Vec<IndexedPhysicalFile>,
    #[serde(default)]
    pub virtual_file_ids: Vec<String>,
    #[serde(default)]
    pub children: BTreeMap<String, GraphNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedPhysicalFile {
    pub virtual_file_id: String,
    pub path: String,
    pub record_count: u64,
}

impl GraphNode {
    pub fn root() -> Self {
        Self {
            value: "__root__".to_string(),
            stats: NodeStats::default(),
            physical_files: Vec::new(),
            virtual_file_ids: Vec::new(),
            children: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataGraph {
    pub dimensions: Vec<String>,
    pub root: GraphNode,
}

impl MetadataGraph {
    pub fn new(dimensions: Vec<String>) -> Result<Self> {
        if dimensions.is_empty() {
            return Err(NemoError::Graph("metadata graph requires at least one dimension".into()));
        }
        for dimension in &dimensions {
            validate_identifier(dimension).map_err(|error| NemoError::Graph(error.to_string()))?;
        }
        Ok(Self {
            dimensions,
            root: GraphNode::root(),
        })
    }

    pub fn insert_virtual_file(
        &mut self,
        virtual_file: &VirtualFile,
        data_files: &[DataFile],
    ) -> Result<()> {
        if data_files.is_empty() {
            return Err(NemoError::Graph("cannot index empty virtual file".into()));
        }
        for data_file in data_files {
            self.insert_data_file(virtual_file, data_file)?;
        }
        Ok(())
    }

    fn insert_data_file(&mut self, virtual_file: &VirtualFile, data_file: &DataFile) -> Result<()> {
        let mut node = &mut self.root;
        node.stats.add_file(data_file);
        for dimension in &self.dimensions {
            let value = data_file.partition_values.get(dimension).ok_or_else(|| {
                NemoError::Graph(format!(
                    "data file {} missing graph dimension {}",
                    data_file.path, dimension
                ))
            })?;
            node = node.children.entry(value.clone()).or_insert_with(|| GraphNode {
                value: value.clone(),
                stats: NodeStats::default(),
                physical_files: Vec::new(),
                virtual_file_ids: Vec::new(),
                children: BTreeMap::new(),
            });
            node.stats.add_file(data_file);
        }
        if !node
            .physical_files
            .iter()
            .any(|file| file.virtual_file_id == virtual_file.id && file.path == data_file.path)
        {
            node.physical_files.push(IndexedPhysicalFile {
                virtual_file_id: virtual_file.id.clone(),
                path: data_file.path.clone(),
                record_count: data_file.record_count,
            });
        }
        if !node.virtual_file_ids.iter().any(|id| id == &virtual_file.id) {
            node.virtual_file_ids.push(virtual_file.id.clone());
            node.stats.add_virtual_file();
        }
        Ok(())
    }

    pub fn plan(
        &self,
        predicates: &BTreeMap<String, String>,
        virtual_files: &BTreeMap<String, VirtualFile>,
        active_virtual_file_ids: &HashSet<String>,
    ) -> Result<QueryPlan> {
        let predicates = predicates
            .iter()
            .map(|(dimension, value)| (dimension.clone(), DimensionPredicate::Equal(value.clone())))
            .collect();
        self.plan_with_predicates(&predicates, virtual_files, active_virtual_file_ids)
    }

    pub fn plan_with_predicates(
        &self,
        predicates: &BTreeMap<String, DimensionPredicate>,
        virtual_files: &BTreeMap<String, VirtualFile>,
        active_virtual_file_ids: &HashSet<String>,
    ) -> Result<QueryPlan> {
        let mut visited_nodes = 1;
        let mut pruned_node_count = 0;
        let mut pruned_physical_file_count = 0;
        let mut frontier = vec![&self.root];

        for dimension in &self.dimensions {
            let mut next = Vec::new();
            if let Some(predicate) = predicates.get(dimension) {
                for node in frontier {
                    for child in node.children.values() {
                        if predicate.matches(&child.value) {
                            visited_nodes += 1;
                            next.push(child);
                        } else {
                            let skipped_nodes = count_nodes(child);
                            pruned_node_count += skipped_nodes;
                            pruned_physical_file_count += child.stats.physical_file_count;
                        }
                    }
                }
            } else {
                for node in frontier {
                    for child in node.children.values() {
                        visited_nodes += 1;
                        next.push(child);
                    }
                }
            }
            frontier = next;
            if frontier.is_empty() {
                break;
            }
        }

        let mut selected_virtual_files = BTreeMap::new();
        for node in frontier {
            collect_virtual_files(node, virtual_files, active_virtual_file_ids, &mut selected_virtual_files)?;
        }

        let selected_physical_file_count = selected_virtual_files
            .values()
            .map(|virtual_file| virtual_file.physical_files.len() as u64)
            .sum();

        Ok(QueryPlan {
            visited_nodes,
            total_indexed_node_count: count_nodes(&self.root),
            pruned_node_count,
            total_indexed_physical_file_count: self.root.stats.physical_file_count,
            pruned_physical_file_count,
            selected_physical_file_count,
            virtual_files: selected_virtual_files.into_values().collect(),
            delete_bitmaps: HashMap::new(),
        })
    }
}

fn count_nodes(node: &GraphNode) -> u64 {
    1 + node.children.values().map(count_nodes).sum::<u64>()
}

fn collect_virtual_files(
    node: &GraphNode,
    virtual_files: &BTreeMap<String, VirtualFile>,
    active_virtual_file_ids: &HashSet<String>,
    output: &mut BTreeMap<String, VirtualFile>,
) -> Result<()> {
    for indexed_file in &node.physical_files {
        if active_virtual_file_ids.contains(&indexed_file.virtual_file_id) {
            if !virtual_files.contains_key(&indexed_file.virtual_file_id) {
                return Err(NemoError::Graph(format!(
                    "Referenced active virtual file ID {} is missing from virtual_files registry",
                    indexed_file.virtual_file_id
                )));
            }
            let entry = output
                .entry(indexed_file.virtual_file_id.clone())
                .or_insert_with(|| VirtualFile {
                    id: indexed_file.virtual_file_id.clone(),
                    physical_files: Vec::new(),
                    record_count: 0,
                });
            if !entry
                .physical_files
                .iter()
                .any(|path| path == &indexed_file.path)
            {
                entry.physical_files.push(indexed_file.path.clone());
                entry.record_count += indexed_file.record_count;
            }
        }
    }
    if node.physical_files.is_empty() {
        for id in &node.virtual_file_ids {
            if active_virtual_file_ids.contains(id) {
                if let Some(virtual_file) = virtual_files.get(id) {
                    output.insert(id.clone(), virtual_file.clone());
                } else {
                    return Err(NemoError::Graph(format!(
                        "Referenced active virtual file ID {} is missing from virtual_files registry",
                        id
                    )));
                }
            }
        }
    }
    for child in node.children.values() {
        collect_virtual_files(child, virtual_files, active_virtual_file_ids, output)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QueryPlan {
    pub visited_nodes: usize,
    pub total_indexed_node_count: u64,
    pub pruned_node_count: u64,
    pub total_indexed_physical_file_count: u64,
    pub pruned_physical_file_count: u64,
    pub selected_physical_file_count: u64,
    pub virtual_files: Vec<VirtualFile>,
    #[serde(default)]
    pub delete_bitmaps: HashMap<String, String>,
}

impl QueryPlan {
    pub fn skipped_physical_file_count(&self) -> u64 {
        self.total_indexed_physical_file_count
            .saturating_sub(self.selected_physical_file_count)
    }

    pub fn skipped_node_count(&self) -> u64 {
        self.pruned_node_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn partitions(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    fn data_file(path: &str, country: &str, date: &str, customer: &str) -> DataFile {
        DataFile::new(
            path,
            10,
            partitions(&[("country", country), ("date", date), ("customer", customer)]),
        )
        .unwrap()
    }

    #[test]
    fn range_predicates_select_matching_dimension_values() {
        let data_files = vec![
            data_file("data/2026-06-24.parquet", "VN", "2026-06-24", "001"),
            data_file("data/2026-06-25.parquet", "VN", "2026-06-25", "001"),
            data_file("data/2026-06-26.parquet", "VN", "2026-06-26", "001"),
            data_file("data/us.parquet", "US", "2026-06-25", "001"),
        ];
        let virtual_file = VirtualFile::from_data_files("vf-1", &data_files).unwrap();
        let mut virtual_files = BTreeMap::new();
        virtual_files.insert(virtual_file.id.clone(), virtual_file.clone());

        let mut graph =
            MetadataGraph::new(vec!["country".into(), "date".into(), "customer".into()]).unwrap();
        graph.insert_virtual_file(&virtual_file, &data_files).unwrap();

        let predicates = [
            ("country".to_string(), DimensionPredicate::equal("VN")),
            (
                "date".to_string(),
                DimensionPredicate::inclusive_range("2026-06-25", "2026-06-26"),
            ),
        ]
        .into_iter()
        .collect();
        let active_ids = virtual_files.keys().cloned().collect();
        let plan = graph.plan_with_predicates(&predicates, &virtual_files, &active_ids).unwrap();

        assert_eq!(plan.selected_physical_file_count, 2);
        assert_eq!(plan.skipped_physical_file_count(), 2);
        assert_eq!(plan.pruned_physical_file_count, 2);
        assert_eq!(plan.virtual_files.len(), 1);
        assert_eq!(
            plan.virtual_files[0].physical_files,
            vec!["data/2026-06-25.parquet", "data/2026-06-26.parquet"]
        );
    }

    #[test]
    fn equality_planning_trims_virtual_files_to_selected_leaf_files() {
        let data_files = vec![
            data_file("data/vn-123.parquet", "VN", "2026-06-25", "123"),
            data_file("data/vn-456.parquet", "VN", "2026-06-25", "456"),
        ];
        let virtual_file = VirtualFile::from_data_files("vf-1", &data_files).unwrap();
        let mut virtual_files = BTreeMap::new();
        virtual_files.insert(virtual_file.id.clone(), virtual_file.clone());

        let mut graph =
            MetadataGraph::new(vec!["country".into(), "date".into(), "customer".into()]).unwrap();
        graph.insert_virtual_file(&virtual_file, &data_files).unwrap();

        let predicates = partitions(&[
            ("country", "VN"),
            ("date", "2026-06-25"),
            ("customer", "123"),
        ]);
        let active_ids = virtual_files.keys().cloned().collect();
        let plan = graph.plan(&predicates, &virtual_files, &active_ids).unwrap();

        assert_eq!(plan.selected_physical_file_count, 1);
        assert_eq!(plan.virtual_files.len(), 1);
        assert_eq!(plan.virtual_files[0].record_count, 10);
        assert_eq!(plan.virtual_files[0].physical_files, vec!["data/vn-123.parquet"]);
    }
}
