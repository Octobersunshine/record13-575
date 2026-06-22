mod hs_database;
mod routes;
mod tax_calculator;

use std::net::SocketAddr;

use axum::Server;
use tower_http::cors::{Any, CorsLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::routes::create_router;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tariff_calculator=info,tower_http=info,axum=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = create_router().layer(cors);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("Server starting, listening at: http://{}", addr);
    tracing::info!("Health check: http://{}/health", addr);
    tracing::info!("API Endpoints:");
    tracing::info!("  POST http://{}/api/tax/calculate - Calculate comprehensive tax", addr);
    tracing::info!("  GET  http://{}/api/category/lookup/:hs_code - Lookup product category", addr);
    tracing::info!("  GET  http://{}/api/categories - List all categories", addr);

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("Failed to start server");
}
