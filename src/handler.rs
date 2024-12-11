//! Request Handlers

use anyhow::Result;
use axum::{
    body::Body,
    extract::{Path, Request},
    http::{header::CONTENT_TYPE, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};

use crate::{counter::Counter, svg, utils::Queries};

#[inline]
#[tracing::instrument]
/// Greeting router
pub(crate) async fn axum_greeting(
    Path(id): Path<String>,
    request: Request,
) -> Result<Response, StatusCode> {
    tracing::debug!("Accepted request.");

    match greeting(id, request).await {
        Ok(greeting) => Ok(greeting),
        Err(error) => {
            tracing::error!("{:?}", error);

            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[inline]
async fn greeting(id: String, request: Request) -> Result<Response> {
    let queries = Queries::try_parse_uri(request.uri());

    let access_key = queries.get("access_key");
    let access_count = Counter::fetch_add(&id, access_key).await;

    // * Custom Timezone, default to Asia/Shanghai
    let tz = queries
        .get("timezone")
        .and_then(|tz| tz.parse().ok())
        .unwrap_or(chrono_tz::Tz::Asia__Shanghai);

    // * Greeting type, can be moe-counter, or default one.
    let greeting_type = queries.get("type").map(AsRef::as_ref);

    let mut content = match greeting_type {
        Some("moe-counter") => {
            todo!()
        }
        _ => {
            // * Note
            let note = queries.get("note");
            svg::GeneralImpl {
                tz,
                access_count,
                note,
            }
            .generate()
            .await
        }
    }
    .into_bytes();

    // ! Avoid unnecessary allocation
    content.shrink_to_fit();

    Response::builder()
        .header(CONTENT_TYPE, HeaderValue::from_static("image/svg+xml"))
        .body(Body::from(bytes::Bytes::from(content)))
        .map_err(Into::into)
}

#[inline]
// TODO: Shutdown connection immediately
pub(crate) async fn fallback(request: Request) -> Response {
    tracing::warn!(?request, "No available handler");

    StatusCode::NOT_FOUND.into_response()
}
