use std::collections::BTreeMap;
use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use nemo_lakehouse::catalog::CatalogTableInfo;
use nemo_lakehouse::graph::DimensionPredicate;
use nemo_lakehouse::{DataFile, LocalCatalog, MetadataGraph, Schema, Table, VirtualFile};

#[derive(Debug, Parser)]
#[command(name = "nemo")]
#[command(about = "Nemo Lakehouse graph-native table format CLI")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Table {
        #[command(subcommand)]
        command: TableCommand,
    },
    Catalog {
        #[command(subcommand)]
        command: CatalogCommand,
    },
    Bench {
        #[command(subcommand)]
        command: BenchCommand,
    },
}

#[derive(Debug, Subcommand)]
enum TableCommand {
    Create {
        path: PathBuf,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long)]
        name: Option<String>,
        #[arg(long = "graph-dim", required = true)]
        graph_dims: Vec<String>,
    },
    Inspect {
        path: PathBuf,
    },
    Append {
        path: PathBuf,
        #[arg(long = "file", required = true)]
        files: Vec<String>,
        #[arg(long)]
        records: u64,
        #[arg(long = "partition", value_parser = parse_key_value)]
        partitions: Vec<(String, String)>,
    },
    History {
        path: PathBuf,
    },
    Plan {
        path: PathBuf,
        #[arg(long = "predicate", value_parser = parse_key_value)]
        predicates: Vec<(String, String)>,
        #[arg(long = "range", value_parser = parse_range_predicate)]
        ranges: Vec<RangePredicate>,
        #[arg(long)]
        snapshot: Option<u64>,
    },
    Compact {
        path: PathBuf,
        #[arg(long = "partition", value_parser = parse_key_value, required = true)]
        partitions: Vec<(String, String)>,
        #[arg(long = "target-file", required = true)]
        target_file: String,
    },
    Delete {
        path: PathBuf,
        #[arg(long = "file", required = true)]
        file: String,
        #[arg(long = "delete-bitmap", required = true)]
        delete_bitmap: String,
    },
    Optimize {
        path: PathBuf,
        #[arg(long)]
        recommend: bool,
    },
}

#[derive(Debug, Subcommand)]
enum CatalogCommand {
    Create {
        warehouse: PathBuf,
        table: String,
        #[arg(long)]
        schema: PathBuf,
        #[arg(long = "graph-dim", required = true)]
        graph_dims: Vec<String>,
    },
    List {
        warehouse: PathBuf,
        #[arg(long)]
        details: bool,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    Inspect {
        warehouse: PathBuf,
        table: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        format: OutputFormat,
    },
    CreateDomain {
        warehouse: PathBuf,
        name: String,
        #[arg(long)]
        description: Option<String>,
        #[arg(long = "rule")]
        rules: Vec<String>,
        #[arg(long = "relation")]
        relations: Vec<String>,
    },
    ListDomains {
        warehouse: PathBuf,
    },
    InspectDomain {
        warehouse: PathBuf,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum BenchCommand {
    Graph {
        #[arg(long, default_value_t = 8)]
        countries: u32,
        #[arg(long, default_value_t = 31)]
        dates: u32,
        #[arg(long, default_value_t = 100)]
        customers: u32,
        #[arg(long = "files-per-leaf", default_value_t = 1)]
        files_per_leaf: u32,
        #[arg(long, default_value = "C001")]
        country: String,
        #[arg(long, default_value = "2026-06-01")]
        date: String,
        #[arg(long, default_value = "cust-000001")]
        customer: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RangePredicate {
    column: String,
    start: String,
    end: String,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Table { command } => run_table(command),
        Command::Catalog { command } => run_catalog(command),
        Command::Bench { command } => run_bench(command),
    }
}

fn run_table(command: TableCommand) -> anyhow::Result<()> {
    match command {
        TableCommand::Create {
            path,
            schema,
            name,
            graph_dims,
        } => {
            let schema = read_schema(schema)?;
            let table_name = name.unwrap_or_else(|| path.file_name().unwrap_or_default().to_string_lossy().into_owned());
            let table = Table::create(path, table_name, schema, graph_dims)?;
            println!("{}", table.path().display());
        }
        TableCommand::Inspect { path } => {
            println!("{}", serde_json::to_string_pretty(&Table::new(path).load_metadata()?)?);
        }
        TableCommand::Append {
            path,
            files,
            records,
            partitions,
        } => {
            let partitions: BTreeMap<String, String> = partitions.into_iter().collect();
            let mut data_files = Vec::with_capacity(files.len());
            for file in files {
                data_files.push(DataFile::new(file, records, partitions.clone())?);
            }
            let snapshot = Table::new(path).append_files(data_files)?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        TableCommand::History { path } => {
            println!("{}", serde_json::to_string_pretty(&Table::new(path).snapshot_history()?)?);
        }
        TableCommand::Plan {
            path,
            predicates,
            ranges,
            snapshot,
        } => {
            let predicates = build_dimension_predicates(predicates, ranges)?;
            let plan = Table::new(path).plan_files_with_predicates_at_snapshot(predicates, snapshot)?;
            let result = serde_json::json!({
                "visited_nodes": plan.visited_nodes,
                "manifest_scan_physical_files": plan.total_indexed_physical_file_count,
                "selected_physical_files": plan.selected_physical_file_count,
                "skipped_physical_files": plan.skipped_physical_file_count(),
                "virtual_files": plan.virtual_files,
                "delete_bitmaps": plan.delete_bitmaps,
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
        TableCommand::Compact {
            path,
            partitions,
            target_file,
        } => {
            let partition_map: BTreeMap<String, String> = partitions.into_iter().collect();
            let snapshot = Table::new(path).compact_files(partition_map, target_file)?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        TableCommand::Delete {
            path,
            file,
            delete_bitmap,
        } => {
            let snapshot = Table::new(path).delete_rows(&file, delete_bitmap)?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        TableCommand::Optimize {
            path,
            recommend,
        } => {
            let table = Table::new(path);
            let result = table.optimize_layout(recommend)?;
            if let Some(dims) = result {
                if recommend {
                    println!("Recommended dimension order: {}", dims.join(", "));
                } else {
                    println!("Optimized dimension order: {}", dims.join(", "));
                }
            } else {
                println!("Dimension order is already optimal or no query history available.");
            }
        }
    }
    Ok(())
}

fn run_bench(command: BenchCommand) -> anyhow::Result<()> {
    match command {
        BenchCommand::Graph {
            countries,
            dates,
            customers,
            files_per_leaf,
            country,
            date,
            customer,
        } => {
            let mut graph = MetadataGraph::new(vec!["country".into(), "date".into(), "customer".into()])?;
            let mut virtual_files = std::collections::BTreeMap::new();
            let mut virtual_file_number = 0_u64;

            for country_idx in 1..=countries {
                let country_value = format!("C{country_idx:03}");
                for date_idx in 1..=dates {
                    let date_value = format!("2026-06-{date_idx:02}");
                    for customer_idx in 1..=customers {
                        let customer_value = format!("cust-{customer_idx:06}");
                        let mut data_files = Vec::new();
                        for file_idx in 1..=files_per_leaf {
                            data_files.push(DataFile::new(
                                format!("data/{country_value}/{date_value}/{customer_value}/part-{file_idx:05}.parquet"),
                                1_000,
                                [
                                    ("country".to_string(), country_value.clone()),
                                    ("date".to_string(), date_value.clone()),
                                    ("customer".to_string(), customer_value.clone()),
                                ]
                                .into_iter()
                                .collect(),
                            )?);
                        }
                        virtual_file_number += 1;
                        let virtual_file = VirtualFile::from_data_files(format!("vf-{virtual_file_number:020}"), &data_files)?;
                        graph.insert_virtual_file(&virtual_file, &data_files)?;
                        virtual_files.insert(virtual_file.id.clone(), virtual_file);
                    }
                }
            }

            let predicates = [
                ("country".to_string(), country),
                ("date".to_string(), date),
                ("customer".to_string(), customer),
            ]
            .into_iter()
            .collect();
            let active_ids = virtual_files.keys().cloned().collect();
            let plan = graph.plan(&predicates, &virtual_files, &active_ids)?;
            let result = serde_json::json!({
                "graph_dimensions": graph.dimensions,
                "visited_nodes": plan.visited_nodes,
                "manifest_scan_physical_files": plan.total_indexed_physical_file_count,
                "selected_physical_files": plan.selected_physical_file_count,
                "skipped_physical_files": plan.skipped_physical_file_count(),
                "selected_virtual_files": plan.virtual_files.len(),
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    }
    Ok(())
}

fn run_catalog(command: CatalogCommand) -> anyhow::Result<()> {
    match command {
        CatalogCommand::Create {
            warehouse,
            table,
            schema,
            graph_dims,
        } => {
            let catalog = LocalCatalog::new(warehouse);
            let table = catalog.create_table(&table, read_schema(schema)?, graph_dims)?;
            println!("{}", table.path().display());
        }
        CatalogCommand::List {
            warehouse,
            details,
            format,
        } => {
            let catalog = LocalCatalog::new(warehouse);
            if details || format == OutputFormat::Json {
                let tables = catalog.list_table_infos()?;
                match format {
                    OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&tables)?),
                    OutputFormat::Text => print_catalog_table(&tables),
                }
            } else {
                for table in catalog.list_tables()? {
                    println!("{table}");
                }
            }
        }
        CatalogCommand::Inspect {
            warehouse,
            table,
            format,
        } => {
            let info = LocalCatalog::new(warehouse).inspect_table(&table)?;
            match format {
                OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&info)?),
                OutputFormat::Text => print_catalog_inspect(&info),
            }
        }
        CatalogCommand::CreateDomain {
            warehouse,
            name,
            description,
            rules,
            relations,
        } => {
            let catalog = LocalCatalog::new(warehouse);
            let mut domain_rules = Vec::new();
            for r_str in rules {
                let parts: Vec<&str> = r_str.splitn(3, ':').collect();
                if parts.len() < 2 {
                    anyhow::bail!("Invalid rule format: '{}'. Expected 'column:type[:config]'", r_str);
                }
                let column_name = parts[0].to_string();
                let rule_type = parts[1];
                let constraint = match rule_type {
                    "NotNull" => nemo_lakehouse::Constraint::NotNull,
                    "MinMax" => {
                        if parts.len() < 3 {
                            anyhow::bail!("MinMax rule requires min,max config: 'column:MinMax:min,max'");
                        }
                        let bounds: Vec<&str> = parts[2].split(',').collect();
                        if bounds.len() != 2 {
                            anyhow::bail!("MinMax config must have exactly 2 parts 'min,max': '{}'", parts[2]);
                        }
                        let min = if bounds[0].is_empty() { None } else { Some(bounds[0].to_string()) };
                        let max = if bounds[1].is_empty() { None } else { Some(bounds[1].to_string()) };
                        nemo_lakehouse::Constraint::MinMax { min, max }
                    }
                    "AllowedValues" => {
                        if parts.len() < 3 {
                            anyhow::bail!("AllowedValues rule requires list of values: 'column:AllowedValues:val1,val2'");
                        }
                        let values = parts[2].split(',').map(|s| s.to_string()).collect();
                        nemo_lakehouse::Constraint::AllowedValues(values)
                    }
                    "RegexMatch" => {
                        if parts.len() < 3 {
                            anyhow::bail!("RegexMatch rule requires pattern config: 'column:RegexMatch:pattern'");
                        }
                        nemo_lakehouse::Constraint::RegexMatch(parts[2].to_string())
                    }
                    _ => anyhow::bail!("Unsupported rule type: '{}'", rule_type),
                };
                domain_rules.push(nemo_lakehouse::domain::DomainRule { column_name, constraint });
            }

            let mut domain_relations = Vec::new();
            for rel_str in relations {
                let parts: Vec<&str> = rel_str.split("->").collect();
                if parts.len() != 2 {
                    anyhow::bail!("Invalid relation format: '{}'. Expected 'table1.col1->table2.col2'", rel_str);
                }
                let from_parts: Vec<&str> = parts[0].split('.').collect();
                let to_parts: Vec<&str> = parts[1].split('.').collect();
                if from_parts.len() != 2 || to_parts.len() != 2 {
                    anyhow::bail!("Invalid relation table/column names in: '{}'", rel_str);
                }
                domain_relations.push(nemo_lakehouse::domain::Relation {
                    from_table: from_parts[0].to_string(),
                    from_column: from_parts[1].to_string(),
                    to_table: to_parts[0].to_string(),
                    to_column: to_parts[1].to_string(),
                });
            }

            let domain = catalog.create_domain(&name, description, domain_rules, domain_relations)?;
            println!("Created domain '{}' under catalog.", domain.name);
        }
        CatalogCommand::ListDomains { warehouse } => {
            let catalog = LocalCatalog::new(warehouse);
            for dom in catalog.list_domains()? {
                println!("{dom}");
            }
        }
        CatalogCommand::InspectDomain { warehouse, name } => {
            let catalog = LocalCatalog::new(warehouse);
            let info = catalog.inspect_domain(&name)?;
            println!("Domain: {}", info.name);
            if let Some(desc) = info.description {
                println!("Description: {}", desc);
            }
            println!("Created At: {}", info.created_at);
            println!("Rules count: {}", info.rules.len());
            for r in &info.rules {
                println!("  - Column '{}': {:?}", r.column_name, r.constraint);
            }
            println!("Relations count: {}", info.relations.len());
            for rel in &info.relations {
                println!("  - Relationship: {}.{} ➔ {}.{}", rel.from_table, rel.from_column, rel.to_table, rel.to_column);
            }
            println!("Tables in domain: {:?}", info.tables);
        }
    }
    Ok(())
}

fn read_schema(path: PathBuf) -> anyhow::Result<Schema> {
    let file = File::open(&path).with_context(|| format!("open schema {}", path.display()))?;
    let schema: Schema = serde_json::from_reader(file).with_context(|| format!("parse schema {}", path.display()))?;
    schema.validate()?;
    Ok(schema)
}

fn parse_key_value(value: &str) -> std::result::Result<(String, String), String> {
    let (key, value) = value
        .split_once('=')
        .ok_or_else(|| "expected key=value".to_string())?;
    Ok((key.to_string(), value.to_string()))
}

fn parse_range_predicate(value: &str) -> std::result::Result<RangePredicate, String> {
    let (column, bounds) = value
        .split_once('=')
        .ok_or_else(|| "expected column=start..end".to_string())?;
    let (start, end) = bounds
        .split_once("..")
        .ok_or_else(|| "expected column=start..end".to_string())?;
    if column.trim().is_empty() || start.trim().is_empty() || end.trim().is_empty() {
        return Err("range predicates require non-empty column, start, and end".to_string());
    }
    if start > end {
        return Err("range predicate start must be less than or equal to end".to_string());
    }
    Ok(RangePredicate {
        column: column.to_string(),
        start: start.to_string(),
        end: end.to_string(),
    })
}

fn build_dimension_predicates(
    equalities: Vec<(String, String)>,
    ranges: Vec<RangePredicate>,
) -> anyhow::Result<BTreeMap<String, DimensionPredicate>> {
    let mut predicates = BTreeMap::new();
    for (dimension, value) in equalities {
        if predicates
            .insert(dimension.clone(), DimensionPredicate::equal(value))
            .is_some()
        {
            anyhow::bail!("duplicate predicate for dimension {dimension}");
        }
    }
    for range in ranges {
        if predicates
            .insert(
                range.column.clone(),
                DimensionPredicate::inclusive_range(range.start, range.end),
            )
            .is_some()
        {
            anyhow::bail!("duplicate predicate for dimension {}", range.column);
        }
    }
    Ok(predicates)
}

fn print_catalog_table(tables: &[CatalogTableInfo]) {
    println!(
        "{:<32} {:>9} {:>9} {:>10} {:>12}  dimensions",
        "table", "snapshot", "snapshots", "files", "records"
    );
    for table in tables {
        let current_snapshot = table
            .current_snapshot_id
            .map(|snapshot_id| snapshot_id.to_string())
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<32} {:>9} {:>9} {:>10} {:>12}  {}",
            table.name,
            current_snapshot,
            table.snapshot_count,
            table.physical_file_count,
            table.record_count,
            table.graph_dimensions.join(",")
        );
    }
}

fn print_catalog_inspect(info: &CatalogTableInfo) {
    println!("table: {}", info.name);
    println!("path: {}", info.path.display());
    println!(
        "current_snapshot_id: {}",
        info.current_snapshot_id
            .map(|snapshot_id| snapshot_id.to_string())
            .unwrap_or_else(|| "-".to_string())
    );
    println!("snapshot_count: {}", info.snapshot_count);
    println!("graph_dimensions: {}", info.graph_dimensions.join(","));
    println!("virtual_file_count: {}", info.virtual_file_count);
    println!("physical_file_count: {}", info.physical_file_count);
    println!("record_count: {}", info.record_count);
}
