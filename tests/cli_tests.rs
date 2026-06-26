use std::process::Command;

use serde_json::Value;

fn nemo() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nemo"))
}

fn write_schema(tempdir: &tempfile::TempDir) -> std::path::PathBuf {
    let schema_path = tempdir.path().join("schema.json");
    std::fs::write(
        &schema_path,
        r#"{
  "fields": [
    { "name": "event_id", "type": "string", "required": true },
    { "name": "country", "type": "string", "required": true },
    { "name": "date", "type": "string", "required": true }
  ]
}
"#,
    )
    .unwrap();
    schema_path
}

#[test]
fn cli_create_append_and_plan_reports_graph_metrics() {
    let tempdir = tempfile::tempdir().unwrap();
    let schema_path = write_schema(&tempdir);
    let table_path = tempdir.path().join("events");

    let create = nemo()
        .args([
            "table",
            "create",
            table_path.to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--name",
            "events",
            "--graph-dim",
            "country",
            "--graph-dim",
            "date",
        ])
        .output()
        .unwrap();
    assert!(
        create.status.success(),
        "create failed: {}",
        String::from_utf8_lossy(&create.stderr)
    );

    let append = nemo()
        .args([
            "table",
            "append",
            table_path.to_str().unwrap(),
            "--file",
            "data/vn.parquet",
            "--records",
            "42",
            "--partition",
            "country=VN",
            "--partition",
            "date=2026-06-25",
        ])
        .output()
        .unwrap();
    assert!(
        append.status.success(),
        "append failed: {}",
        String::from_utf8_lossy(&append.stderr)
    );

    let plan = nemo()
        .args([
            "table",
            "plan",
            table_path.to_str().unwrap(),
            "--predicate",
            "country=VN",
            "--predicate",
            "date=2026-06-25",
        ])
        .output()
        .unwrap();
    assert!(
        plan.status.success(),
        "plan failed: {}",
        String::from_utf8_lossy(&plan.stderr)
    );

    let plan: Value = serde_json::from_slice(&plan.stdout).unwrap();
    assert_eq!(plan["visited_nodes"], 3);
    assert_eq!(plan["manifest_scan_physical_files"], 1);
    assert_eq!(plan["selected_physical_files"], 1);
    assert_eq!(plan["skipped_physical_files"], 0);
    assert_eq!(
        plan["virtual_files"][0]["physical_files"],
        serde_json::json!(["data/vn.parquet"])
    );
}

#[test]
fn cli_catalog_create_rejects_traversal_table_name() {
    let tempdir = tempfile::tempdir().unwrap();
    let schema_path = write_schema(&tempdir);

    let output = nemo()
        .args([
            "catalog",
            "create",
            tempdir.path().join("warehouse").to_str().unwrap(),
            "../events",
            "--schema",
            schema_path.to_str().unwrap(),
            "--graph-dim",
            "country",
        ])
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid table name"));
}

#[test]
fn cli_features_integration() {
    let tempdir = tempfile::tempdir().unwrap();
    let schema_path = write_schema(&tempdir);
    let table_path = tempdir.path().join("events");

    // 1. Create table
    nemo()
        .args([
            "table",
            "create",
            table_path.to_str().unwrap(),
            "--schema",
            schema_path.to_str().unwrap(),
            "--graph-dim",
            "country",
            "--graph-dim",
            "date",
        ])
        .output()
        .unwrap();

    // 2. Append files
    nemo()
        .args([
            "table",
            "append",
            table_path.to_str().unwrap(),
            "--file",
            "data/vn-1.parquet",
            "--records",
            "10",
            "--partition",
            "country=VN",
            "--partition",
            "date=2026-06-25",
        ])
        .output()
        .unwrap();

    nemo()
        .args([
            "table",
            "append",
            table_path.to_str().unwrap(),
            "--file",
            "data/vn-2.parquet",
            "--records",
            "20",
            "--partition",
            "country=VN",
            "--partition",
            "date=2026-06-25",
        ])
        .output()
        .unwrap();

    // 3. Plan at Snapshot 1 (Time travel)
    let plan_s1_output = nemo()
        .args([
            "table",
            "plan",
            table_path.to_str().unwrap(),
            "--predicate",
            "country=VN",
            "--predicate",
            "date=2026-06-25",
            "--snapshot",
            "1",
        ])
        .output()
        .unwrap();
    let plan_s1: Value = serde_json::from_slice(&plan_s1_output.stdout).unwrap();
    assert_eq!(plan_s1["selected_physical_files"], 1);
    assert_eq!(plan_s1["virtual_files"][0]["physical_files"][0], "data/vn-1.parquet");

    // 4. Compact files
    let compact_output = nemo()
        .args([
            "table",
            "compact",
            table_path.to_str().unwrap(),
            "--partition",
            "country=VN",
            "--partition",
            "date=2026-06-25",
            "--target-file",
            "data/vn-compacted.parquet",
        ])
        .output()
        .unwrap();
    assert!(compact_output.status.success());

    // 5. Plan at Snapshot 3 (Post compaction)
    let plan_s3_output = nemo()
        .args([
            "table",
            "plan",
            table_path.to_str().unwrap(),
            "--predicate",
            "country=VN",
            "--predicate",
            "date=2026-06-25",
        ])
        .output()
        .unwrap();
    let plan_s3: Value = serde_json::from_slice(&plan_s3_output.stdout).unwrap();
    assert_eq!(plan_s3["selected_physical_files"], 1);
    assert_eq!(plan_s3["virtual_files"][0]["physical_files"][0], "data/vn-compacted.parquet");

    // 6. Delete rows (Delete bitmap)
    let delete_output = nemo()
        .args([
            "table",
            "delete",
            table_path.to_str().unwrap(),
            "--file",
            "data/vn-compacted.parquet",
            "--delete-bitmap",
            "paths/bitmap.bin",
        ])
        .output()
        .unwrap();
    assert!(delete_output.status.success());

    // 7. Plan at Snapshot 4 (Post delete)
    let plan_s4_output = nemo()
        .args([
            "table",
            "plan",
            table_path.to_str().unwrap(),
            "--predicate",
            "country=VN",
            "--predicate",
            "date=2026-06-25",
        ])
        .output()
        .unwrap();
    let plan_s4: Value = serde_json::from_slice(&plan_s4_output.stdout).unwrap();
    assert_eq!(plan_s4["delete_bitmaps"]["data/vn-compacted.parquet"], "paths/bitmap.bin");

    // 8. Optimize layout
    let optimize_output = nemo()
        .args([
            "table",
            "optimize",
            table_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(optimize_output.status.success());
    let stdout = String::from_utf8_lossy(&optimize_output.stdout);
    assert!(stdout.contains("Optimized dimension order") || stdout.contains("already optimal"));
}
