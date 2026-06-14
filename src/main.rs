mod handlers;
mod util;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Json},
    http::Method,
    routing::{delete, get, post},
};
use dotenvy::dotenv;
use lru::LruCache;
use serde::Serialize;
use std::{env, num::NonZeroUsize, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{Mutex, Semaphore},
};
use tower_http::cors::{Any, CorsLayer};

use handlers::{
    serve_image::get_image,
    upload::{generate_upload_url, handle_upload},
};

use crate::handlers::delete_image::delete_image;

pub const MAX_SIZE: usize = 10; // 10 MB is the maximum size
pub const UPLOAD_URL_EXPIRATION: u64 = 15 * 60; // 15 minutes in seconds
pub const CACHE_EXPIRATION: u64 = 60 * 60; // 1 hour in seconds
pub const CACHE_DIR: &str = ".cache";
pub const UPLOAD_DIR: &str = "uploads";
pub const CACHE_TRACKER_SIZE: usize = 500; // Track up to 500 cached items

#[derive(Serialize)]
struct HelloWorld {
    message: String,
}

#[derive(Clone)]
pub struct AppState {
    pub cache_tracker: Arc<Mutex<LruCache<String, ()>>>,
    pub conversion_limit: Arc<Semaphore>,
    pub resize_limit: Arc<Semaphore>,
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to listen for shutdown signal");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("Exiting...");
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let cors = CorsLayer::new().allow_origin(Any).allow_methods([
        Method::GET,
        Method::POST,
        Method::DELETE,
    ]);

    let cache_limit = NonZeroUsize::new(CACHE_TRACKER_SIZE).unwrap(); // Track up to 500 cached items

    let app_state = AppState {
        conversion_limit: Arc::new(Semaphore::new(4)),
        resize_limit: Arc::new(Semaphore::new(8)),
        cache_tracker: Arc::new(Mutex::new(LruCache::new(cache_limit))),
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
        .route("/image/{filename}/delete-image", delete(delete_image))
        .with_state(app_state)
        .layer(DefaultBodyLimit::max(MAX_SIZE * 1024 * 1024))
        .layer(cors);

    let port = env::var("PORT").expect("No Port Number Found");
    let address = env::var("ADDRESS").expect("No Address Found");

    let socket_addr = format!("{}:{}", address, port);

    let listner: TcpListener = TcpListener::bind(&socket_addr).await.unwrap();

    println!("Server running on http://{}", socket_addr);

    tokio::fs::create_dir_all(UPLOAD_DIR).await.unwrap();
    tokio::fs::create_dir_all(CACHE_DIR).await.unwrap();

    axum::serve(listner, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    //clear cache on shutdown
    tokio::fs::remove_dir_all(CACHE_DIR).await.unwrap();
}
