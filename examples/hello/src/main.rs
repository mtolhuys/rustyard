use axum::{response::Html, routing::get, Router};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("PORT").unwrap_or_else(|_| "3100".into());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    println!("Rustyard example listening on {}", listener.local_addr()?);
    let app = Router::new()
        .route("/", get(|| async { Html("<h1>Hello from Rustyard</h1><p>Edit src/main.rs. Rustyard recompiles and restarts this app.</p>") }))
        .route("/health", get(|| async { "ok" }));
    axum::serve(listener, app).await?;
    Ok(())
}
