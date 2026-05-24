use std::io::Cursor;

use axum::{
    extract::{Path, Query},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct ImageParams {
    w: Option<u32>,
    h: Option<u32>,
    ext: Option<String>,
}

pub async fn get_image(
    Path(filename): Path<String>,
    Query(params): Query<ImageParams>,
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

    let img = image::load_from_memory(&raw_bytes).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            String::from("Error Parsing Image"),
        )
    })?;

    let target_w = params.w.unwrap_or(img.width());
    let target_h = params.h.unwrap_or(img.height());
    let target_ext = params.ext.as_deref().unwrap_or("webp");

    let cache_path = format!(
        "cache/{}-{}-{}-{}",
        filename, target_w, target_h, target_ext
    );

    let content_type = if target_ext == "jpeg" {
        "image/jpeg"
    } else {
        "image/webp"
    };

    if tokio::fs::metadata(&cache_path).await.is_ok() {
        let cache_bytes = tokio::fs::read(&cache_path).await.unwrap();
        headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
        headers.insert("x-cache", HeaderValue::from_static("HIT"));

        return Ok((headers, cache_bytes));
    }

    let resized_img = img.resize_exact(target_w, target_h, image::imageops::FilterType::Nearest);

    let mut buffer = Cursor::new(Vec::new());
    if target_ext == "jpeg" {
        resized_img
            .write_to(&mut buffer, image::ImageFormat::Jpeg)
            .unwrap();
    } else {
        resized_img
            .write_to(&mut buffer, image::ImageFormat::WebP)
            .unwrap();
    }

    let final_bytes = buffer.into_inner();

    tokio::fs::write(&cache_path, &final_bytes).await.unwrap();

    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert("x-cache", HeaderValue::from_static("MISS"));

    return Ok((headers, final_bytes));
}
