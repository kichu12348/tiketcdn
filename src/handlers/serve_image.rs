use std::io::Cursor;

use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;
use tokio::task::spawn_blocking;

#[derive(Deserialize)]
pub(crate) struct ImageParams {
    w: Option<u32>,
    h: Option<u32>,
    ext: Option<String>,
}

pub async fn get_image(
    Path(filename): Path<String>,
    Query(params): Query<ImageParams>,
    State(app_state): State<AppState>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let file_path = format!("uploads/{}", filename);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    headers.insert("Server", HeaderValue::from_static("tiketcdn"));

    let raw_bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, String::from("Image not found!")))?;

    if params.w.is_none() && params.h.is_none() && params.ext.is_none() {
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("image/webp"));
        headers.insert("x-cache", HeaderValue::from_static("HIT"));
        return Ok((headers, raw_bytes));
    }

    let reader = image::ImageReader::new(Cursor::new(&raw_bytes))
        .with_guessed_format()
        .unwrap();
    let (orig_w, orig_h) = reader.into_dimensions().unwrap();

    let target_w = params.w.unwrap_or(orig_w);
    let target_h = params.h.unwrap_or(orig_h);

    if target_h <= 0 || target_w <= 0 {
        return Err((StatusCode::BAD_REQUEST, String::from("Invalid dimensions!")));
    }

    let target_ext = params.ext.as_deref().unwrap_or("webp").to_ascii_lowercase();

    let cache_path = format!(
        "cache/{}-{}-{}-{}",
        filename, target_w, target_h, target_ext
    );

    let content_type = if target_ext.as_str() == "jpeg" {
        "image/jpeg"
    } else {
        "image/webp"
    };

    if tokio::fs::metadata(&cache_path).await.is_ok() {
        let cache_bytes = tokio::fs::read(&cache_path).await.unwrap();
        {
            let mut tracker = app_state.cache_tracker.lock().await;
            tracker.put(cache_path.clone(), ());
        }

        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert("x-cache", HeaderValue::from_static("HIT"));

        return Ok((headers, cache_bytes));
    }

    let _persist = app_state.resize_limit.acquire().await.unwrap();

    let final_bytes = spawn_blocking(move || {
        let img = image::load_from_memory(&raw_bytes).unwrap();
        let resized_img =
            img.resize_exact(target_w, target_h, image::imageops::FilterType::Triangle);

        let mut buffer = Cursor::new(Vec::new());
        if target_ext.as_str() == "jpeg" {
            resized_img
                .write_to(&mut buffer, image::ImageFormat::Jpeg)
                .unwrap();
        } else {
            resized_img
                .write_to(&mut buffer, image::ImageFormat::WebP)
                .unwrap();
        }

        return buffer.into_inner();
    })
    .await
    .unwrap();

    tokio::fs::write(&cache_path, &final_bytes).await.unwrap();

    {
        let mut tracker = app_state.cache_tracker.lock().await;
        if let Some((evicted_path, _)) = tracker.push(cache_path.clone(), ()) {
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(evicted_path).await;
            });
        }
    }

    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert("x-cache", HeaderValue::from_static("MISS"));

    return Ok((headers, final_bytes));
}
