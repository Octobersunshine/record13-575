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
    tracing::info!("  POST http://{}/api/tax/calculate - Calculate comprehensive tax with classification risks", addr);
    tracing::info!("  GET  http://{}/api/category/lookup/:hs_code - Smart classify with alternatives and risk detection", addr);
    tracing::info!("  GET  http://{}/api/categories - List all HS categories", addr);
    tracing::info!("  POST http://{}/api/batch/consistency-check - Check batch classification consistency", addr);

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .expect("Failed to start server");
}
