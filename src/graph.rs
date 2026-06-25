use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::{NemoError, Result};
use crate::metadata::{DataFile, VirtualFile};
use crate::schema::validate_identifier;

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
    pub virtual_file_ids: Vec<String>,
    #[serde(default)]
    pub children: BTreeMap<String, GraphNode>,
}

impl GraphNode {
    pub fn root() -> Self {
        Self {
            value: "__root__".to_string(),
            stats: NodeStats::default(),
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

    pub fn insert_virtual_file(&mut self, virtual_file: &VirtualFile, data_files: &[DataFile]) -> Result<()> {
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
                virtual_file_ids: Vec::new(),
                children: BTreeMap::new(),
            });
            node.stats.add_file(data_file);
        }
        if !node.virtual_file_ids.iter().any(|id| id == &virtual_file.id) {
            node.virtual_file_ids.push(virtual_file.id.clone());
            node.stats.add_virtual_file();
        }
        Ok(())
    }

    pub fn plan(&self, predicates: &BTreeMap<String, String>, virtual_files: &BTreeMap<String, VirtualFile>) -> QueryPlan {
        let mut visited_nodes = 1;
        let mut frontier = vec![&self.root];

        for dimension in &self.dimensions {
            let mut next = Vec::new();
            if let Some(value) = predicates.get(dimension) {
                for node in frontier {
                    if let Some(child) = node.children.get(value) {
                        visited_nodes += 1;
                        next.push(child);
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
            collect_virtual_files(node, virtual_files, &mut selected_virtual_files);
        }

        QueryPlan {
            visited_nodes,
            virtual_files: selected_virtual_files.into_values().cloned().collect(),
        }
    }
}

fn collect_virtual_files<'a>(
    node: &GraphNode,
    virtual_files: &'a BTreeMap<String, VirtualFile>,
    output: &mut BTreeMap<String, &'a VirtualFile>,
) {
    for id in &node.virtual_file_ids {
        if let Some(virtual_file) = virtual_files.get(id) {
            output.insert(id.clone(), virtual_file);
        }
    }
    for child in node.children.values() {
        collect_virtual_files(child, virtual_files, output);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryPlan {
    pub visited_nodes: usize,
    pub virtual_files: Vec<VirtualFile>,
}
