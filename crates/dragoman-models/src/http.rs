// SPDX-FileCopyrightText: 2026 Blagovest Petrov <blagovest@petrovs.info>
// SPDX-FileCopyrightText: 2026 Vute Tech Ltd. <https://vute.tech>
// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal HTTP fetch abstraction, so the provider logic can be tested
//! offline against a fake implementation.

use std::future::Future;
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("request to {url} failed: {message}")]
    Request { url: String, message: String },
    #[error("{url} returned status {status}")]
    Status { url: String, status: u16 },
}

#[derive(Debug)]
pub enum FetchResult {
    /// The server said the cached copy (per `if_none_match`) is current.
    NotModified,
    Fetched {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

pub trait Http: Send + Sync {
    fn get(
        &self,
        url: &str,
        if_none_match: Option<&str>,
    ) -> impl Future<Output = Result<FetchResult, HttpError>> + Send;
}

/// The real client. HTTPS only (the URLs we build all are).
pub struct ReqwestHttp {
    client: reqwest::Client,
}

impl ReqwestHttp {
    pub fn new() -> Self {
        ReqwestHttp {
            client: reqwest::Client::builder()
                .user_agent(concat!("dragomand/", env!("CARGO_PKG_VERSION")))
                .https_only(true)
                .build()
                .expect("reqwest client builds"),
        }
    }
}

impl Default for ReqwestHttp {
    fn default() -> Self {
        Self::new()
    }
}

impl Http for ReqwestHttp {
    async fn get(&self, url: &str, if_none_match: Option<&str>) -> Result<FetchResult, HttpError> {
        let mut request = self.client.get(url);
        if let Some(etag) = if_none_match {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(|e| HttpError::Request {
            url: url.to_owned(),
            message: e.to_string(),
        })?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(FetchResult::NotModified);
        }
        if !response.status().is_success() {
            return Err(HttpError::Status {
                url: url.to_owned(),
                status: response.status().as_u16(),
            });
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let bytes = response
            .bytes()
            .await
            .map_err(|e| HttpError::Request {
                url: url.to_owned(),
                message: e.to_string(),
            })?
            .to_vec();
        Ok(FetchResult::Fetched { bytes, etag })
    }
}

/// GET with ETag revalidation backed by `<cache_dir>/<name>.json` and
/// `<name>.etag`. When the network fails, a stale cached copy beats
/// nothing at all.
pub async fn cached_get<H: Http>(
    http: &H,
    url: &str,
    cache_dir: &Path,
    name: &str,
) -> Result<Vec<u8>, HttpError> {
    let body_path = cache_dir.join(format!("{name}.json"));
    let etag_path = cache_dir.join(format!("{name}.etag"));
    let cached_body = std::fs::read(&body_path).ok();
    let cached_etag = std::fs::read_to_string(&etag_path).ok();

    let etag = cached_body
        .is_some()
        .then_some(cached_etag.as_deref())
        .flatten();
    match http.get(url, etag).await {
        Ok(FetchResult::NotModified) => {
            Ok(cached_body.expect("etag was only sent with a cached body"))
        }
        Ok(FetchResult::Fetched { bytes, etag }) => {
            let _ = std::fs::create_dir_all(cache_dir);
            let _ = std::fs::write(&body_path, &bytes);
            match etag {
                Some(etag) => {
                    let _ = std::fs::write(&etag_path, etag);
                }
                None => {
                    let _ = std::fs::remove_file(&etag_path);
                }
            }
            Ok(bytes)
        }
        Err(error) => match cached_body {
            Some(bytes) => Ok(bytes),
            None => Err(error),
        },
    }
}
