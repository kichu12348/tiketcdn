mod handlers;
mod util;
use axum::{
    Router,
    extract::{DefaultBodyLimit, Json},
    routing::{get, post},
};
use dotenvy::dotenv;
use lru::LruCache;
use serde::Serialize;
use std::{env, num::NonZeroUsize, sync::Arc};
use tokio::{
    net::TcpListener,
    sync::{Mutex, Semaphore},
};

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
    pub cache_tracker: Arc<Mutex<LruCache<String, ()>>>,
    pub conversion_limit: Arc<Semaphore>,
    pub resize_limit: Arc<Semaphore>,
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for shutdown signal");

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>(); // On non-Unix platforms, just wait indefinitely

    #[cfg(unix)]
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    #[cfg(not(unix))]
    {
        let _ = ctrl_c;
        let _ = terminate;
    }

    println!("Exiting...");
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let cache_limit = NonZeroUsize::new(5000).unwrap(); // Track up to 5000 cached items

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
        .with_state(app_state)
        .layer(DefaultBodyLimit::max(MAX_SIZE * 1024 * 1024));

    let port = env::var("PORT").expect("No Port Number Found");
    let address = env::var("ADDRESS").expect("No Address Found");

    let socket_addr = format!("{}:{}", address, port);

    let listner: TcpListener = TcpListener::bind(&socket_addr).await.unwrap();

    println!("Server running on http://{}", socket_addr);

    tokio::fs::create_dir_all("uploads").await.unwrap();
    tokio::fs::create_dir_all("cache").await.unwrap();

    axum::serve(listner, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    //clear cache on shutdown
    tokio::fs::remove_dir_all("cache").await.unwrap();
}
