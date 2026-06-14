use crate::{AppState, CACHE_DIR, UPLOAD_DIR};
use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};
use serde::Serialize;
use tokio::fs;

#[derive(Serialize)]
pub(crate) struct DeleteImageResponse {
    message: String,
}

#[derive(Serialize)]
pub(crate) struct ErrorResponse {
    error: String,
}

pub async fn delete_image(
    State(app_state): State<AppState>,
    Path(filename): Path<String>,
) -> Result<(StatusCode, Json<DeleteImageResponse>), (StatusCode, Json<ErrorResponse>)> {
    let _ = app_state;
    let image_path = format!("{}/{}", UPLOAD_DIR, filename);

    fs::remove_file(&image_path).await.map_err(|_| {
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "Image not found".to_string(),
            }),
        )
    })?;

    if let Ok(mut entries) = fs::read_dir(CACHE_DIR).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_name_os = entry.file_name();
            let file_name = file_name_os.to_string_lossy().to_string();

            if file_name.starts_with(&filename) {
                let cache_file_path = format!("{}/{}", CACHE_DIR, file_name);
                let _ = fs::remove_file(&cache_file_path).await;

                let mut cache_tracker = app_state.cache_tracker.lock().await;
                cache_tracker.pop(&cache_file_path);
            }
        }
    }

    Ok((
        StatusCode::OK,
        Json(DeleteImageResponse {
            message: "Image deleted successfully".to_string(),
        }),
    ))
}
