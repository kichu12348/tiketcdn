use axum::{
    Json,
    extract::{Multipart, Query, State},
    http::StatusCode,
};

use tokio::io::AsyncWriteExt;

use crate::{
    AppState, MAX_SIZE,
    util::{now::now_secs, sign::create_signature},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
    State(app_state): State<AppState>,
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

    while let Some(mut field) = multipart.next_field().await.map_err(|err| {
        (
            StatusCode::BAD_REQUEST,
            Json(UploadErrorResponse {
                error: format!("Upload Error: {}", err),
            }),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        if name == "file" {
            let temp_id = Uuid::new_v4().simple().to_string();
            let temp_path = format!("uploads/temp_{}", temp_id);
            let mut temp_file = tokio::fs::File::create(&temp_path).await.map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(UploadErrorResponse {
                        error: format!("File Write Error: {}", err),
                    }),
                )
            })?;

            let mut total_size: usize = 0;

            while let Some(chunk) = field.chunk().await.map_err(|err| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(UploadErrorResponse {
                        error: format!("File Read Error: {}", err),
                    }),
                )
            })? {
                total_size += chunk.len();

                if total_size > MAX_SIZE * 1024 * 1024 {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        Json(UploadErrorResponse {
                            error: String::from("File too large"),
                        }),
                    ));
                }
                temp_file.write_all(&chunk).await.map_err(|err| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(UploadErrorResponse {
                            error: format!("File Write Error: {}", err),
                        }),
                    )
                })?;
            }

            drop(temp_file);

            let webp_filename = Uuid::new_v4().simple().to_string();
            let final_path = format!("uploads/{}.webp", webp_filename);

            let _permit = app_state.conversion_limit.acquire().await.unwrap();

            let t_path = temp_path.clone();
            let f_path = final_path.clone();

            let final_result = tokio::task::spawn_blocking(move || {
                let img = image::ImageReader::open(&t_path)
                    .map_err(|err| format!("Image Processing Error: {}", err))?
                    .with_guessed_format()
                    .map_err(|err| format!("Image Processing Error: {}", err))?
                    .decode()
                    .map_err(|err| format!("Image Processing Error: {}", err))?;

                img.save_with_format(&f_path, image::ImageFormat::WebP)
                    .map_err(|err| format!("Image Saving Error: {}", err))?;

                Ok::<(), String>(())
            })
            .await
            .unwrap();

            let _ = tokio::fs::remove_file(&temp_path).await;

            if let Err(e) = final_result {
                let _ = tokio::fs::remove_file(&final_path).await;
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(UploadErrorResponse { error: e }),
                ));
            }

            file_name = format!("{}.webp", webp_filename);
        }
    }

    Ok(Json(UploadResponse {
        filename: file_name,
    }))
}
