use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;

use crate::tax_calculator::{ErrorResponse, TaxCalculateRequest, TaxCalculator};

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    message: String,
}

async fn health_check() -> impl IntoResponse {
    let response = HealthResponse {
        status: "ok".to_string(),
        message: "Tariff calculator service is running".to_string(),
    };
    (StatusCode::OK, Json(response))
}

async fn calculate_tax(Json(payload): Json<TaxCalculateRequest>) -> impl IntoResponse {
    match TaxCalculator::calculate(&payload) {
        Ok(result) => (StatusCode::OK, Json(serde_json::to_value(result).unwrap())).into_response(),
        Err(err) => {
            let error = ErrorResponse {
                error: "Calculation failed".to_string(),
                message: err,
            };
            (StatusCode::BAD_REQUEST, Json(serde_json::to_value(error).unwrap())).into_response()
        }
    }
}

async fn lookup_category(Path(hs_code): Path<String>) -> impl IntoResponse {
    match crate::hs_database::HsDatabase::lookup(&hs_code) {
        Some(category) => (StatusCode::OK, Json(serde_json::to_value(category).unwrap())).into_response(),
        None => {
            let error = ErrorResponse {
                error: "Not found".to_string(),
                message: format!("Unrecognized HS code: {}", hs_code),
            };
            (StatusCode::NOT_FOUND, Json(serde_json::to_value(error).unwrap())).into_response()
        }
    }
}

async fn list_all_categories() -> impl IntoResponse {
    let categories = TaxCalculator::list_categories();
    (StatusCode::OK, Json(serde_json::to_value(categories).unwrap()))
}

pub fn create_router() -> Router {
    Router::new()
        .route("/health", get(health_check))
        .route("/api/tax/calculate", post(calculate_tax))
        .route("/api/category/lookup/:hs_code", get(lookup_category))
        .route("/api/categories", get(list_all_categories))
}
