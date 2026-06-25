use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use nemo_lakehouse::{DataFile, LocalCatalog, Schema, Table};

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

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Table { command } => run_table(command),
        Command::Catalog { command } => run_catalog(command),
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
                "virtual_files": plan.virtual_files,
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
