use crate::get;
use axum::extract::{Path, RawQuery};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect};
use axum::routing::get;
use axum::Router;
use std::net::SocketAddr;

#[tokio::main]
pub async fn main() -> Result<String, String> {
    // build our application with a route
    let app = Router::new()
        // `GET /` goes to `root`
        .route("/", get(root))
        .route("/:table", get(table));

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    let hello = String::from("Hello, world!");
    Ok(hello)
}

async fn root() -> impl IntoResponse {
    tracing::info!("request root");
    Redirect::permanent("/table")
}

async fn table(Path(table): Path<String>, RawQuery(query): RawQuery) -> impl IntoResponse {
    tracing::info!("request table {:?} {:?}", table, query);
    match get::table(table).await {
        Ok(html) => (StatusCode::FOUND, Html(html)),
        Err(_) => (StatusCode::NOT_FOUND, Html("404 Not Found".to_string())),
    }
}
