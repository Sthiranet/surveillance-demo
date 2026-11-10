//! `surveillance-director` serves the demo: `cargo run -p surveillance-director`, then open http://localhost:3000.
//!
//! - `PORT` sets the port (default 3000).
//! - `STHIRA_UI_DIST` points at the built UI (default `demo/ui/dist`).
//! - `surveillance-director --openapi` prints the OpenAPI spec and exits.

use std::path::PathBuf;

use surveillance_director::World;
use surveillance_director::api::{ApiDoc, app};
use utoipa::OpenApi;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    if std::env::args().any(|a| a == "--openapi") {
        println!("{}", ApiDoc::openapi().to_pretty_json().expect("spec serialises"));
        return Ok(());
    }

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(3000);
    let dist = std::env::var_os("STHIRA_UI_DIST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../ui/dist"));
    if !dist.join("index.html").exists() {
        eprintln!("warning: no UI build at {}. Run `npm run build` in demo/ui (the API still works).", dist.display());
    }

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("Sthiranet surveillance demo running at http://localhost:{port}");
    axum::serve(listener, app(World::shared(), &dist))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
}
