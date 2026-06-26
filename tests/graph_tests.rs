use std::collections::BTreeMap;

use nemo_lakehouse::{DataFile, Field, FieldType, NemoError, Schema, Table};

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
    assert_eq!(plan.total_indexed_physical_file_count, 2);
    assert_eq!(plan.selected_physical_file_count, 1);
    assert_eq!(plan.skipped_physical_file_count(), 1);
    assert!(plan.visited_nodes < 6);
}

#[test]
fn graph_equality_plan_reports_pruning_metrics() {
    let tempdir = tempfile::tempdir().unwrap();
    let table = Table::create(
        tempdir.path().join("events"),
        "events",
        schema(),
        vec!["country".into(), "date".into(), "customer".into()],
    )
    .unwrap();

    table
        .append_files(vec![
            DataFile::new(
                "data/vn-123.parquet",
                100,
                partitions(&[("country", "VN"), ("date", "2026-06-25"), ("customer", "123")]),
            )
            .unwrap(),
            DataFile::new(
                "data/vn-456.parquet",
                200,
                partitions(&[("country", "VN"), ("date", "2026-06-25"), ("customer", "456")]),
            )
            .unwrap(),
            DataFile::new(
                "data/us-123.parquet",
                300,
                partitions(&[("country", "US"), ("date", "2026-06-25"), ("customer", "123")]),
            )
            .unwrap(),
        ])
        .unwrap();

    let plan = table
        .plan_files(partitions(&[
            ("country", "VN"),
            ("date", "2026-06-25"),
            ("customer", "123"),
        ]))
        .unwrap();

    assert_eq!(plan.visited_nodes, 4);
    assert_eq!(plan.total_indexed_physical_file_count, 3);
    assert_eq!(plan.selected_physical_file_count, 1);
    assert_eq!(plan.skipped_physical_file_count(), 2);
    assert_eq!(plan.virtual_files.len(), 1);
    assert_eq!(plan.virtual_files[0].physical_files, vec!["data/vn-123.parquet"]);
}

#[test]
fn graph_plan_for_unknown_equality_predicate_scans_no_files() {
    let tempdir = tempfile::tempdir().unwrap();
    let table = Table::create(
        tempdir.path().join("events"),
        "events",
        schema(),
        vec!["country".into(), "date".into()],
    )
    .unwrap();

    table
        .append_files(vec![DataFile::new(
            "data/vn.parquet",
            100,
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
        )
        .unwrap()])
        .unwrap();

    let plan = table
        .plan_files(partitions(&[("country", "TH"), ("date", "2026-06-25")]))
        .unwrap();

    assert_eq!(plan.visited_nodes, 1);
    assert_eq!(plan.total_indexed_physical_file_count, 1);
    assert_eq!(plan.selected_physical_file_count, 0);
    assert_eq!(plan.skipped_physical_file_count(), 1);
    assert!(plan.virtual_files.is_empty());
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
    assert_eq!(virtual_file.physical_files, vec!["data/a.parquet", "data/b.parquet"]);
    assert_eq!(virtual_file.record_count, 30);
    assert_eq!(virtual_file.physical_files.len(), 2);
}

#[test]
fn virtual_file_grouping_preserves_multiple_commits_at_same_leaf() {
    let tempdir = tempfile::tempdir().unwrap();
    let table = Table::create(
        tempdir.path().join("events"),
        "events",
        schema(),
        vec!["country".into(), "date".into()],
    )
    .unwrap();

    table
        .append_files(vec![DataFile::new(
            "data/a.parquet",
            10,
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
        )
        .unwrap()])
        .unwrap();
    table
        .append_files(vec![DataFile::new(
            "data/b.parquet",
            20,
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
        )
        .unwrap()])
        .unwrap();

    let plan = table
        .plan_files(partitions(&[("country", "VN"), ("date", "2026-06-25")]))
        .unwrap();

    assert_eq!(plan.virtual_files.len(), 2);
    assert_eq!(plan.selected_physical_file_count, 2);
    assert_eq!(plan.virtual_files[0].physical_files, vec!["data/a.parquet"]);
    assert_eq!(plan.virtual_files[1].physical_files, vec!["data/b.parquet"]);
}

#[test]
fn append_rejects_files_missing_graph_dimensions() {
    let tempdir = tempfile::tempdir().unwrap();
    let table = Table::create(
        tempdir.path().join("events"),
        "events",
        schema(),
        vec!["country".into(), "date".into()],
    )
    .unwrap();

    let error = table
        .append_files(vec![DataFile::new(
            "data/missing-date.parquet",
            10,
            partitions(&[("country", "VN")]),
        )
        .unwrap()])
        .unwrap_err();

    assert!(
        matches!(error, NemoError::Graph(message) if message.contains("missing graph dimension date"))
    );
    assert!(table.snapshot_history().unwrap().is_empty());
    assert!(table.load_metadata().unwrap().virtual_files.is_empty());
}
