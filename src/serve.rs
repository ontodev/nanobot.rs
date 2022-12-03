use crate::get;
use axum::extract::{Path, RawQuery};
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

async fn root() -> String {
    get::table(String::from("table"))
}

async fn table(Path(table): Path<String>, RawQuery(query): RawQuery) -> String {
    tracing::info!("query {:?}", query);
    get::table(table)
}
