use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::{NemoError, Result};
use crate::schema::Schema;
use crate::table::Table;

#[derive(Debug, Clone)]
pub struct LocalCatalog {
    warehouse: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogTableInfo {
    pub name: String,
    pub path: PathBuf,
    pub current_snapshot_id: Option<u64>,
    pub snapshot_count: usize,
    pub graph_dimensions: Vec<String>,
    pub virtual_file_count: usize,
    pub physical_file_count: u64,
    pub record_count: u64,
    pub graph: crate::graph::MetadataGraph,
    pub virtual_files: std::collections::BTreeMap<String, crate::metadata::VirtualFile>,
    pub schema: crate::schema::Schema,
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

    pub fn inspect_table(&self, table_name: &str) -> Result<CatalogTableInfo> {
        let table = self.load_table(table_name)?;
        let metadata = table.load_metadata()?;
        let snapshots = table.snapshot_history()?;
        
        let name = metadata.table_name;
        let schema = metadata.schema.clone();
        let graph_dimensions = metadata.graph.dimensions.clone();
        let virtual_file_count = metadata.virtual_files.len();
        let physical_file_count = metadata.graph.root.stats.physical_file_count;
        let record_count = metadata.graph.root.stats.row_count;
        let graph = metadata.graph;
        let virtual_files = metadata.virtual_files;

        Ok(CatalogTableInfo {
            name,
            path: table.path().to_path_buf(),
            current_snapshot_id: metadata.current_snapshot_id,
            snapshot_count: snapshots.len(),
            graph_dimensions,
            virtual_file_count,
            physical_file_count,
            record_count,
            graph,
            virtual_files,
            schema,
        })
    }

    pub fn list_table_infos(&self) -> Result<Vec<CatalogTableInfo>> {
        self.list_tables()?
            .into_iter()
            .map(|table_name| self.inspect_table(&table_name))
            .collect()
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

#[derive(Debug, Clone, Serialize)]
pub struct CatalogDomainInfo {
    pub name: String,
    pub description: Option<String>,
    pub rules: Vec<crate::domain::DomainRule>,
    pub relations: Vec<crate::domain::Relation>,
    pub tables: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CatalogTreeNode {
    pub name: String,
    pub path: String,
    pub node_type: String, // "root", "domain", "table", "folder"
    pub children: Vec<CatalogTreeNode>,
}

impl LocalCatalog {
    pub fn create_domain(
        &self,
        name: &str,
        description: Option<String>,
        rules: Vec<crate::domain::DomainRule>,
        relations: Vec<crate::domain::Relation>,
    ) -> Result<crate::domain::DomainMetadata> {
        let domain_path = self.warehouse.join(name.replace('.', "/"));
        let meta_dir = domain_path.join("_nemo_domain");
        fs::create_dir_all(&meta_dir)?;

        let metadata = crate::domain::DomainMetadata::new(
            name.to_string(),
            description,
            rules,
            relations,
        );

        let meta_file = meta_dir.join("domain.json");
        let writer = fs::File::create(meta_file)?;
        serde_json::to_writer_pretty(writer, &metadata)?;

        Ok(metadata)
    }

    pub fn load_domain(&self, name: &str) -> Result<crate::domain::DomainMetadata> {
        let domain_path = self.warehouse.join(name.replace('.', "/"));
        let meta_file = domain_path.join("_nemo_domain").join("domain.json");
        if !meta_file.exists() {
            return Err(NemoError::Validation(format!("Domain '{}' not found", name)));
        }
        let reader = fs::File::open(meta_file)?;
        let metadata: crate::domain::DomainMetadata = serde_json::from_reader(reader)?;
        Ok(metadata)
    }

    pub fn list_domains(&self) -> Result<Vec<String>> {
        let mut domains = Vec::new();
        if !self.warehouse.exists() {
            return Ok(domains);
        }
        collect_domains(&self.warehouse, &self.warehouse, &mut domains)?;
        domains.sort();
        Ok(domains)
    }

    pub fn inspect_domain(&self, name: &str) -> Result<CatalogDomainInfo> {
        let domain = self.load_domain(name)?;
        
        let domain_path = self.warehouse.join(name.replace('.', "/"));
        let mut tables = Vec::new();
        if domain_path.exists() {
            collect_tables(&domain_path, &domain_path, &mut tables)?;
        }
        let tables = tables
            .into_iter()
            .map(|t| if name.is_empty() { t } else { format!("{}.{}", name, t) })
            .collect();

        Ok(CatalogDomainInfo {
            name: domain.name,
            description: domain.description,
            rules: domain.rules,
            relations: domain.relations,
            tables,
            created_at: domain.created_at,
            updated_at: domain.updated_at,
        })
    }

    pub fn catalog_tree(&self) -> Result<CatalogTreeNode> {
        let root_node = build_tree_node(&self.warehouse, &self.warehouse)?;
        Ok(root_node.unwrap_or_else(|| CatalogTreeNode {
            name: "warehouse".to_string(),
            path: "".to_string(),
            node_type: "root".to_string(),
            children: Vec::new(),
        }))
    }
}

fn collect_domains(root: &Path, current: &Path, output: &mut Vec<String>) -> Result<()> {
    if current.join("_nemo_domain").join("domain.json").exists() {
        let relative = current.strip_prefix(root).map_err(|error| NemoError::Metadata(error.to_string()))?;
        output.push(
            relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("."),
        );
    }
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let dir_name = entry.file_name();
            if dir_name == "_nemo" || dir_name == "_nemo_domain" {
                continue;
            }
            collect_domains(root, &entry.path(), output)?;
        }
    }
    Ok(())
}

fn build_tree_node(root: &Path, current: &Path) -> Result<Option<CatalogTreeNode>> {
    let relative = current.strip_prefix(root).map_err(|error| NemoError::Metadata(error.to_string()))?;
    let path_str = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join(".");

    // Check if it is a Table
    if current.join("_nemo").join("metadata.json").exists() {
        return Ok(Some(CatalogTreeNode {
            name: current.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            path: path_str,
            node_type: "table".to_string(),
            children: Vec::new(),
        }));
    }

    // Check if it is a Domain or Root
    let is_domain = current.join("_nemo_domain").join("domain.json").exists();
    let is_root = relative.as_os_str().is_empty();

    if is_domain || is_root {
        let name = if is_root {
            "warehouse".to_string()
        } else {
            current.file_name().unwrap_or_default().to_string_lossy().into_owned()
        };

        let mut children = Vec::new();
        if current.exists() {
            for entry in fs::read_dir(current)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    let dir_name = entry.file_name();
                    if dir_name == "_nemo" || dir_name == "_nemo_domain" {
                        continue;
                    }
                    if let Some(child_node) = build_tree_node(root, &entry.path())? {
                        children.push(child_node);
                    }
                }
            }
        }
        children.sort_by(|a, b| a.name.cmp(&b.name));

        return Ok(Some(CatalogTreeNode {
            name,
            path: path_str,
            node_type: if is_root { "root".to_string() } else { "domain".to_string() },
            children,
        }));
    }

    let mut children = Vec::new();
    if current.exists() {
        for entry in fs::read_dir(current)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let dir_name = entry.file_name();
                if dir_name == "_nemo" || dir_name == "_nemo_domain" {
                    continue;
                }
                if let Some(child_node) = build_tree_node(root, &entry.path())? {
                    children.push(child_node);
                }
            }
        }
    }

    if !children.is_empty() {
        children.sort_by(|a, b| a.name.cmp(&b.name));
        return Ok(Some(CatalogTreeNode {
            name: current.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            path: path_str,
            node_type: "folder".to_string(),
            children,
        }));
    }

    Ok(None)
}
