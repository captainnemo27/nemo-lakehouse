use nemo_lakehouse::{Field, FieldType, Schema};

#[test]
fn schema_rejects_duplicate_fields() {
    let result = Schema::new(vec![
        Field::new("id", FieldType::String, true),
        Field::new("id", FieldType::Long, false),
    ]);

    assert!(result.is_err());
}

#[test]
fn schema_rejects_path_like_field_names() {
    let result = Schema::new(vec![Field::new("../id", FieldType::String, true)]);

    assert!(result.is_err());
}

