use std::collections::BTreeMap;

use nemo_lakehouse::{DataFile, Field, FieldType, Schema, Table};

fn schema() -> Schema {
    Schema::new(vec![
        Field::new("event_id", FieldType::String, true),
        Field::new("country", FieldType::String, true),
        Field::new("date", FieldType::String, true),
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
fn test_time_travel_and_compaction_and_delete_bitmaps() {
    let tempdir = tempfile::tempdir().unwrap();
    let table_path = tempdir.path().join("events");
    let table = Table::create(
        &table_path,
        "events",
        schema(),
        vec!["country".into(), "date".into()],
    )
    .unwrap();

    // 1. Snapshot 1: Append first file
    table
        .append_files(vec![DataFile::new(
            "data/vn-1.parquet",
            100,
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
        )
        .unwrap()])
        .unwrap();

    // 2. Snapshot 2: Append second file to the same partition
    table
        .append_files(vec![DataFile::new(
            "data/vn-2.parquet",
            200,
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
        )
        .unwrap()])
        .unwrap();

    // Validate Snapshot 1 planning (Time Travel)
    let plan_s1 = table
        .plan_files_at_snapshot(
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
            Some(1),
        )
        .unwrap();
    assert_eq!(plan_s1.selected_physical_file_count, 1);
    assert_eq!(plan_s1.virtual_files.len(), 1);
    assert_eq!(plan_s1.virtual_files[0].physical_files, vec!["data/vn-1.parquet"]);
    assert_eq!(plan_s1.virtual_files[0].record_count, 100);

    // Validate Snapshot 2 planning (Latest)
    let plan_s2 = table
        .plan_files(partitions(&[("country", "VN"), ("date", "2026-06-25")]))
        .unwrap();
    assert_eq!(plan_s2.selected_physical_file_count, 2);
    assert_eq!(plan_s2.virtual_files.len(), 2);

    // 3. Snapshot 3: Run Compaction
    let snapshot_compact = table
        .compact_files(
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
            "data/vn-compacted.parquet".to_string(),
        )
        .unwrap();
    assert_eq!(snapshot_compact.snapshot_id, 3);

    // Validate Snapshot 3 planning (Compacted)
    let plan_s3 = table
        .plan_files(partitions(&[("country", "VN"), ("date", "2026-06-25")]))
        .unwrap();
    assert_eq!(plan_s3.selected_physical_file_count, 1);
    assert_eq!(plan_s3.virtual_files.len(), 1);
    assert_eq!(plan_s3.virtual_files[0].physical_files, vec!["data/vn-compacted.parquet"]);
    assert_eq!(plan_s3.virtual_files[0].record_count, 300); // 100 + 200

    // Validate Time Travel back to Snapshot 2 is unaffected by Compaction
    let plan_s2_post = table
        .plan_files_at_snapshot(
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
            Some(2),
        )
        .unwrap();
    assert_eq!(plan_s2_post.selected_physical_file_count, 2);

    // 4. Snapshot 4: Row-Level Delete (Delete Bitmap)
    table
        .delete_rows("data/vn-compacted.parquet", "paths/delete_bitmap.bin".to_string())
        .unwrap();

    // Validate Snapshot 4 planning has delete bitmap
    let plan_s4 = table
        .plan_files(partitions(&[("country", "VN"), ("date", "2026-06-25")]))
        .unwrap();
    assert_eq!(plan_s4.selected_physical_file_count, 1);
    assert_eq!(
        plan_s4.delete_bitmaps.get("data/vn-compacted.parquet"),
        Some(&"paths/delete_bitmap.bin".to_string())
    );

    // Validate Snapshot 3 planning does NOT have delete bitmap (Time Travel isolation)
    let plan_s3_post = table
        .plan_files_at_snapshot(
            partitions(&[("country", "VN"), ("date", "2026-06-25")]),
            Some(3),
        )
        .unwrap();
    assert!(plan_s3_post.delete_bitmaps.is_empty());
}

#[test]
fn test_adaptive_graph_optimizer() {
    let tempdir = tempfile::tempdir().unwrap();
    let table_path = tempdir.path().join("events");
    let table = Table::create(
        &table_path,
        "events",
        schema(),
        vec!["country".into(), "date".into(), "customer".into()],
    )
    .unwrap();

    table
        .append_files(vec![DataFile::new(
            "data/vn.parquet",
            100,
            partitions(&[("country", "VN"), ("date", "2026-06-25"), ("customer", "123")]),
        )
        .unwrap()])
        .unwrap();

    // Record queries to build history
    // Query 1: date only
    table.record_query(&["date".to_string()]).unwrap();
    // Query 2: date only
    table.record_query(&["date".to_string()]).unwrap();
    // Query 3: date and country
    table.record_query(&["date".to_string(), "country".to_string()]).unwrap();

    // Run optimize layout dry run
    let recommendation = table.optimize_layout(true).unwrap();
    assert_eq!(recommendation, Some(vec!["date".into(), "country".into(), "customer".into()]));

    // Dimensions in metadata should still be unchanged
    let metadata_before = table.load_metadata().unwrap();
    assert_eq!(metadata_before.graph.dimensions, vec!["country", "date", "customer"]);

    // Run layout optimization for real
    let optimized = table.optimize_layout(false).unwrap();
    assert_eq!(optimized, Some(vec!["date".into(), "country".into(), "customer".into()]));

    // Dimensions in metadata must now be reordered
    let metadata_after = table.load_metadata().unwrap();
    assert_eq!(metadata_after.graph.dimensions, vec!["date", "country", "customer"]);

    // Planning should still work correctly and visit correct nodes
    let plan = table
        .plan_files(partitions(&[("country", "VN"), ("date", "2026-06-25")]))
        .unwrap();
    assert_eq!(plan.selected_physical_file_count, 1);
    assert_eq!(plan.virtual_files[0].physical_files, vec!["data/vn.parquet"]);
}

#[test]
fn test_domain_governance_rules() {
    let tempdir = tempfile::tempdir().unwrap();
    let catalog = nemo_lakehouse::catalog::LocalCatalog::new(tempdir.path());

    // 1. Create a Domain with rules:
    // - column "event_id": NotNull
    // - column "country": AllowedValues("VN", "US")
    // - column "date": RegexMatch("^[0-9]{4}-[0-9]{2}$")
    let rules = vec![
        nemo_lakehouse::domain::DomainRule {
            column_name: "event_id".to_string(),
            constraint: nemo_lakehouse::Constraint::NotNull,
        },
        nemo_lakehouse::domain::DomainRule {
            column_name: "country".to_string(),
            constraint: nemo_lakehouse::Constraint::AllowedValues(vec!["VN".to_string(), "US".to_string()]),
        },
        nemo_lakehouse::domain::DomainRule {
            column_name: "date".to_string(),
            constraint: nemo_lakehouse::Constraint::RegexMatch("^[0-9]{4}-[0-9]{2}$".to_string()),
        },
    ];

    let relations = vec![
        nemo_lakehouse::domain::Relation {
            from_table: "events".to_string(),
            from_column: "customer_id".to_string(),
            to_table: "customers".to_string(),
            to_column: "id".to_string(),
        }
    ];

    let domain = catalog.create_domain("finance", Some("Financial records".to_string()), rules, relations).unwrap();
    assert_eq!(domain.name, "finance");
    assert_eq!(domain.rules.len(), 3);

    // 2. Create Table under Domain
    let schema = Schema::new(vec![
        Field::new("event_id", FieldType::String, true),
        Field::new("country", FieldType::String, true),
        Field::new("date", FieldType::String, true),
    ]).unwrap();

    let table = catalog.create_table("finance.events", schema, vec!["country".to_string(), "date".to_string()]).unwrap();

    // 3. Append Valid File (should succeed)
    let valid_file = DataFile::new(
        "data/valid.parquet",
        100,
        partitions(&[("country", "VN"), ("date", "2026-06")]),
    ).unwrap();

    let result = table.append_files(vec![valid_file]);
    assert!(result.is_ok());

    // 4. Append Invalid File - Country not allowed (should fail)
    let invalid_country = DataFile::new(
        "data/invalid_country.parquet",
        100,
        partitions(&[("country", "SG"), ("date", "2026-06")]),
    ).unwrap();
    let result_country = table.append_files(vec![invalid_country]);
    assert!(result_country.is_err());
    let err_msg = result_country.unwrap_err().to_string();
    assert!(err_msg.contains("AllowedValues"));

    // 5. Append Invalid File - Date format wrong (should fail)
    let invalid_date = DataFile::new(
        "data/invalid_date.parquet",
        100,
        partitions(&[("country", "VN"), ("date", "2026-06-25")]),
    ).unwrap();
    let result_date = table.append_files(vec![invalid_date]);
    assert!(result_date.is_err());
    let err_msg_date = result_date.unwrap_err().to_string();
    assert!(err_msg_date.contains("RegexMatch"));

    // 6. Test Catalog Tree
    let tree = catalog.catalog_tree().unwrap();
    assert_eq!(tree.name, "warehouse");
    assert_eq!(tree.children.len(), 1); // "finance" domain
    assert_eq!(tree.children[0].name, "finance");
    assert_eq!(tree.children[0].node_type, "domain");
    assert_eq!(tree.children[0].children.len(), 1); // "events" table
    assert_eq!(tree.children[0].children[0].name, "events");
    assert_eq!(tree.children[0].children[0].node_type, "table");
}
