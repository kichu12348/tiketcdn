mod handlers;
mod util;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Json},
    routing::{get, post},
};
use dotenvy::dotenv;
use serde::Serialize;
use std::{env, sync::Arc};
use tokio::{net::TcpListener, sync::Semaphore};

use handlers::{
    serve_image::get_image,
    upload::{generate_upload_url, handle_upload},
};

pub const MAX_SIZE: usize = 15; // 15 MB is the maximum size

#[derive(Serialize)]
struct HelloWorld {
    message: String,
}

#[derive(Clone)]
pub struct AppState {
    pub conversion_limit: Arc<Semaphore>,
    pub resize_limit: Arc<Semaphore>
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let app_state = AppState {
        conversion_limit: Arc::new(Semaphore::new(4)),
        resize_limit: Arc::new(Semaphore::new(8))
    };

    let app: Router = Router::new()
        .route(
            "/",
            get(async || {
                return Json(HelloWorld {
                    message: String::from("Hellow dis is tiketcdn"),
                });
            }),
        )
        .route("/generate-url", post(generate_upload_url))
        .route("/upload", post(handle_upload))
        .route("/image/{filename}", get(get_image))
        .with_state(app_state)
        .layer(DefaultBodyLimit::max(MAX_SIZE * 1024 * 1024));

    let port = env::var("PORT").expect("No Port Number Found");
    let address = env::var("ADDRESS").expect("No Address Found");

    let socket_addr = format!("{}:{}", address, port);

    let listner: TcpListener = TcpListener::bind(&socket_addr).await.unwrap();

    println!("Server running on http://{}", socket_addr);

    tokio::fs::create_dir_all("uploads").await.unwrap();
    tokio::fs::create_dir_all("cache").await.unwrap();

    axum::serve(listner, app).await.unwrap();
}
