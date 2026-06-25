use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
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
        #[arg(long)]
        file: String,
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
            file,
            records,
            partitions,
        } => {
            let snapshot = Table::new(path).append_files(vec![DataFile::new(file, records, partitions.into_iter().collect())?])?;
            println!("{}", serde_json::to_string_pretty(&snapshot)?);
        }
        TableCommand::History { path } => {
            println!("{}", serde_json::to_string_pretty(&Table::new(path).snapshot_history()?)?);
        }
        TableCommand::Plan { path, predicates } => {
            let plan = Table::new(path).plan_files(predicates.into_iter().collect())?;
            let result = serde_json::json!({
                "visited_nodes": plan.visited_nodes,
                "manifest_scan_physical_files": plan.total_indexed_physical_file_count,
                "selected_physical_files": plan.selected_physical_file_count,
                "skipped_physical_files": plan.skipped_physical_file_count(),
                "virtual_files": plan.virtual_files,
            });
            println!("{}", serde_json::to_string_pretty(&result)?);
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
            let plan = graph.plan(&predicates, &virtual_files);
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
        CatalogCommand::List { warehouse } => {
            for table in LocalCatalog::new(warehouse).list_tables()? {
                println!("{table}");
            }
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
