use std::io::Cursor;

use axum::{
    Json,
    extract::{Multipart, Query},
    http::StatusCode,
};

use crate::util::{now::now_secs, sign::create_signature};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_SIZE: usize = 50;

#[derive(Deserialize)]
pub(crate) struct UploadUrlRequest {
    filename: String,
}

#[derive(Serialize)]
pub(crate) struct UploadUrlResponse {
    url: String,
    max_size: u8,
}

#[derive(Deserialize)]
pub(crate) struct UploadRequest {
    filename: String,
    expires: u64,
    signature: String,
}

#[derive(Serialize)]
pub(crate) struct UploadResponse {
    filename: String,
}

#[derive(Serialize)]
pub(crate) struct UploadErrorResponse {
    error: String,
}

pub async fn generate_upload_url(Json(payload): Json<UploadUrlRequest>) -> Json<UploadUrlResponse> {
    let expires = now_secs() + 3600;

    let signature = create_signature(&payload.filename, expires);

    let upload_url = format!(
        "/upload?filename={}&expires={}&signature={}",
        payload.filename, expires, signature
    );

    return Json(UploadUrlResponse {
        url: upload_url,
        max_size: MAX_SIZE as u8,
    });
}

pub async fn handle_upload(
    Query(params): Query<UploadRequest>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, Json<UploadErrorResponse>)> {
    if now_secs() > params.expires {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(UploadErrorResponse {
                error: String::from("Url Has Expired"),
            }),
        ));
    }
    let expected_signature = create_signature(&params.filename, params.expires);
    if expected_signature != params.signature {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(UploadErrorResponse {
                error: String::from("Nice try boi, Invalid Signature"),
            }),
        ));
    }

    let mut file_name = params.filename;

    while let Some(field) = multipart.next_field().await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(UploadErrorResponse {
                error: format!("Upload Error: {}", err),
            }),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let data = field.bytes().await.unwrap();

            if data.len() > MAX_SIZE * 1024 * 1024 {
                return Err((
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(UploadErrorResponse {
                        error: String::from("File too large"),
                    }),
                ));
            }

            let img = image::load_from_memory(&data).map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(UploadErrorResponse {
                        error: format!("File Cannot be Parsed: {}", err),
                    }),
                )
            })?;

            let rgba_img = img.to_rgba8();

            let mut buffer = Cursor::new(Vec::new());

            image::DynamicImage::ImageRgba8(rgba_img.clone())
                .write_to(&mut buffer, image::ImageFormat::WebP)
                .map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(UploadErrorResponse {
                            error: format!("File Cannot be Parsed: {}", err),
                        }),
                    )
                })?;

            let webp_filename = Uuid::new_v4().simple();

            let file_path = format!("uploads/{}.webp", webp_filename);

            tokio::fs::write(&file_path, buffer.into_inner())
                .await
                .unwrap();
            file_name = format!("{}.webp", webp_filename);
        }
    }

    Ok(Json(UploadResponse {
        filename: file_name,
    }))
}
