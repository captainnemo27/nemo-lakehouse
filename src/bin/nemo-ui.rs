use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nemo_lakehouse::catalog::CatalogTableInfo;
use nemo_lakehouse::graph::DimensionPredicate;
use nemo_lakehouse::metadata::{DataFile, Snapshot};
use nemo_lakehouse::{Field, FieldType, LocalCatalog, Schema};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

const WAREHOUSE_DIR: &str = "./warehouse";

#[derive(Clone)]
struct AppState {
    catalog: LocalCatalog,
}

#[tokio::main]
async fn main() {
    let state = AppState {
        catalog: LocalCatalog::new(WAREHOUSE_DIR),
    };

    // Build API router
    let api_router = Router::new()
        .route("/tables", get(list_tables))
        .route("/table/create", post(create_table))
        .route("/table/:name", get(inspect_table))
        .route("/table/:name/history", get(table_history))
        .route("/table/:name/append", post(append_files))
        .route("/table/:name/plan", post(plan_files))
        .route("/table/:name/compact", post(compact_files))
        .route("/table/:name/delete", post(delete_rows))
        .route("/table/:name/optimize", post(optimize_layout))
        .route("/catalog/tree", get(catalog_tree))
        .route("/domains", get(list_domains))
        .route("/domain/create", post(create_domain))
        .route("/domain/:name", get(inspect_domain))
        .with_state(state);

    // Serve static files from the "ui" directory
    let app = Router::new()
        .nest("/api", api_router)
        .fallback_service(ServeDir::new("ui"))
        .layer(CorsLayer::permissive());

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    println!("Nemo Lakehouse Rust UI Server running at http://localhost:8000");
    axum::serve(listener, app).await.unwrap();
}

// --- API Handlers ---

async fn list_tables(State(state): State<AppState>) -> Result<Json<Vec<CatalogTableInfo>>, AppError> {
    let tables = state.catalog.list_table_infos()?;
    Ok(Json(tables))
}

async fn inspect_table(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<CatalogTableInfo>, AppError> {
    let info = state.catalog.inspect_table(&name)?;
    Ok(Json(info))
}

async fn table_history(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<Vec<Snapshot>>, AppError> {
    let table = state.catalog.load_table(&name)?;
    let history = table.snapshot_history()?;
    Ok(Json(history))
}

#[derive(Deserialize)]
struct CreateTablePayload {
    table_name: String,
    schema_fields: Vec<SchemaFieldInput>,
    graph_dims: Vec<String>,
}

#[derive(Deserialize)]
struct SchemaFieldInput {
    name: String,
    #[serde(rename = "type")]
    field_type: String,
    required: bool,
}

async fn create_table(
    State(state): State<AppState>,
    Json(payload): Json<CreateTablePayload>,
) -> Result<Json<HashMap<String, String>>, AppError> {
    let mut fields = Vec::with_capacity(payload.schema_fields.len());
    for f in payload.schema_fields {
        let field_type = match f.field_type.as_str() {
            "boolean" => FieldType::Boolean,
            "int" => FieldType::Int,
            "long" => FieldType::Long,
            "float" => FieldType::Float,
            "double" => FieldType::Double,
            "string" => FieldType::String,
            "binary" => FieldType::Binary,
            "date" => FieldType::Date,
            "timestamp" => FieldType::Timestamp,
            _ => {
                return Err(AppError(
                    StatusCode::BAD_REQUEST,
                    format!("Unsupported field type: {}", f.field_type),
                ))
            }
        };
        fields.push(Field::new(f.name, field_type, f.required));
    }

    let schema = Schema::new(fields)?;
    let table = state
        .catalog
        .create_table(&payload.table_name, schema, payload.graph_dims)?;

    let mut response = HashMap::new();
    response.insert("table_path".to_string(), table.path().to_string_lossy().to_string());
    Ok(Json(response))
}

#[derive(Deserialize)]
struct AppendPayload {
    files: Vec<String>,
    records: u64,
    partitions: BTreeMap<String, String>,
}

async fn append_files(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<AppendPayload>,
) -> Result<Json<Snapshot>, AppError> {
    let table = state.catalog.load_table(&name)?;
    let mut data_files = Vec::with_capacity(payload.files.len());
    for file in payload.files {
        data_files.push(DataFile::new(file, payload.records, payload.partitions.clone())?);
    }
    let snapshot = table.append_files(data_files)?;
    Ok(Json(snapshot))
}

#[derive(Deserialize)]
struct PlanPayload {
    predicates: BTreeMap<String, String>,
    ranges: BTreeMap<String, (String, String)>,
    snapshot: Option<u64>,
}

#[derive(Serialize)]
struct PlanOutput {
    visited_nodes: usize,
    manifest_scan_physical_files: u64,
    selected_physical_files: u64,
    skipped_physical_files: u64,
    virtual_files: Vec<nemo_lakehouse::metadata::VirtualFile>,
    delete_bitmaps: HashMap<String, String>,
}

async fn plan_files(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<PlanPayload>,
) -> Result<Json<PlanOutput>, AppError> {
    let table = state.catalog.load_table(&name)?;
    let mut dims_predicates = BTreeMap::new();
    for (k, v) in payload.predicates {
        dims_predicates.insert(k, DimensionPredicate::equal(v));
    }
    for (k, (start, end)) in payload.ranges {
        dims_predicates.insert(k, DimensionPredicate::inclusive_range(start, end));
    }

    let plan = table.plan_files_with_predicates_at_snapshot(dims_predicates, payload.snapshot)?;
    Ok(Json(PlanOutput {
        visited_nodes: plan.visited_nodes,
        manifest_scan_physical_files: plan.total_indexed_physical_file_count,
        selected_physical_files: plan.selected_physical_file_count,
        skipped_physical_files: plan.skipped_physical_file_count(),
        virtual_files: plan.virtual_files,
        delete_bitmaps: plan.delete_bitmaps,
    }))
}

#[derive(Deserialize)]
struct CompactPayload {
    partitions: BTreeMap<String, String>,
    target_file: String,
}

async fn compact_files(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<CompactPayload>,
) -> Result<Json<Snapshot>, AppError> {
    let table = state.catalog.load_table(&name)?;
    let snapshot = table.compact_files(payload.partitions, payload.target_file)?;
    Ok(Json(snapshot))
}

#[derive(Deserialize)]
struct DeletePayload {
    file: String,
    delete_bitmap: String,
}

async fn delete_rows(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<DeletePayload>,
) -> Result<Json<Snapshot>, AppError> {
    let table = state.catalog.load_table(&name)?;
    let snapshot = table.delete_rows(&payload.file, payload.delete_bitmap)?;
    Ok(Json(snapshot))
}

#[derive(Deserialize)]
struct OptimizePayload {
    recommend: bool,
}

#[derive(Serialize)]
struct OptimizeOutput {
    message: String,
}

async fn optimize_layout(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(payload): Json<OptimizePayload>,
) -> Result<Json<OptimizeOutput>, AppError> {
    let table = state.catalog.load_table(&name)?;
    let result = table.optimize_layout(payload.recommend)?;
    let message = match result {
        Some(dims) => {
            if payload.recommend {
                format!("Recommended dimension order: {}", dims.join(", "))
            } else {
                format!("Optimized dimension order: {}", dims.join(", "))
            }
        }
        None => "Dimension order is already optimal or no query history available.".to_string(),
    };
    Ok(Json(OptimizeOutput { message }))
}

// --- Domain Handlers ---

async fn catalog_tree(State(state): State<AppState>) -> Result<Json<nemo_lakehouse::catalog::CatalogTreeNode>, AppError> {
    let tree = state.catalog.catalog_tree()?;
    Ok(Json(tree))
}

async fn list_domains(State(state): State<AppState>) -> Result<Json<Vec<String>>, AppError> {
    let domains = state.catalog.list_domains()?;
    Ok(Json(domains))
}

#[derive(Deserialize)]
struct CreateDomainPayload {
    name: String,
    description: Option<String>,
    rules: Vec<nemo_lakehouse::domain::DomainRule>,
    relations: Vec<nemo_lakehouse::domain::Relation>,
}

async fn create_domain(
    State(state): State<AppState>,
    Json(payload): Json<CreateDomainPayload>,
) -> Result<Json<nemo_lakehouse::domain::DomainMetadata>, AppError> {
    let metadata = state.catalog.create_domain(
        &payload.name,
        payload.description,
        payload.rules,
        payload.relations,
    )?;
    Ok(Json(metadata))
}

async fn inspect_domain(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Result<Json<nemo_lakehouse::catalog::CatalogDomainInfo>, AppError> {
    let info = state.catalog.inspect_domain(&name)?;
    Ok(Json(info))
}

// --- Error Handling Wrapper ---

struct AppError(StatusCode, String);

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = Json(serde_json::json!({ "error": self.1 }));
        (self.0, body).into_response()
    }
}

impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        let err: anyhow::Error = err.into();
        Self(StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    }
}
