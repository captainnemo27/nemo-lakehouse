use nemo_lakehouse::{Field, FieldType, LocalCatalog, Schema};

#[test]
fn catalog_creates_and_lists_tables() {
    let tempdir = tempfile::tempdir().unwrap();
    let catalog = LocalCatalog::new(tempdir.path());
    let schema = Schema::new(vec![Field::new("id", FieldType::String, true)]).unwrap();

    catalog
        .create_table("analytics.events", schema, vec!["country".into()])
        .unwrap();

    assert_eq!(catalog.list_tables().unwrap(), vec!["analytics.events"]);
}

#[test]
fn catalog_rejects_traversal_names() {
    let catalog = LocalCatalog::new("/tmp/warehouse");

    assert!(catalog.table_path("../events").is_err());
}

