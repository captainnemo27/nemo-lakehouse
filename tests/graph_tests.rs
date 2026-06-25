use std::collections::BTreeMap;

use nemo_lakehouse::{DataFile, Field, FieldType, Schema, Table};

fn schema() -> Schema {
    Schema::new(vec![
        Field::new("event_id", FieldType::String, true),
        Field::new("country", FieldType::String, true),
        Field::new("customer", FieldType::String, false),
    ])
    .unwrap()
}

fn partitions(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn graph_plans_by_predicate_path() {
    let tempdir = tempfile::tempdir().unwrap();
    let table = Table::create(
        tempdir.path().join("events"),
        "events",
        schema(),
        vec!["country".into(), "date".into(), "customer".into()],
    )
    .unwrap();

    table
        .append_files(vec![DataFile::new(
            "data/vn.parquet",
            100,
            partitions(&[("country", "VN"), ("date", "2026-06"), ("customer", "123")]),
        )
        .unwrap()])
        .unwrap();
    table
        .append_files(vec![DataFile::new(
            "data/us.parquet",
            50,
            partitions(&[("country", "US"), ("date", "2026-06"), ("customer", "999")]),
        )
        .unwrap()])
        .unwrap();

    let plan = table
        .plan_files(partitions(&[("country", "VN"), ("date", "2026-06")]))
        .unwrap();

    assert_eq!(plan.virtual_files.len(), 1);
    assert_eq!(plan.virtual_files[0].physical_files, vec!["data/vn.parquet"]);
    assert!(plan.visited_nodes < 6);
}

#[test]
fn virtual_file_groups_small_files_without_rewrite() {
    let tempdir = tempfile::tempdir().unwrap();
    let table = Table::create(
        tempdir.path().join("events"),
        "events",
        schema(),
        vec!["country".into(), "date".into()],
    )
    .unwrap();

    table
        .append_files(vec![
            DataFile::new("data/a.parquet", 10, partitions(&[("country", "VN"), ("date", "2026-06")])).unwrap(),
            DataFile::new("data/b.parquet", 20, partitions(&[("country", "VN"), ("date", "2026-06")])).unwrap(),
        ])
        .unwrap();

    let metadata = table.load_metadata().unwrap();
    let virtual_file = metadata.virtual_files.values().next().unwrap();
    assert_eq!(virtual_file.record_count, 30);
    assert_eq!(virtual_file.physical_files.len(), 2);
}

