// SPDX-License-Identifier: GPL-3.0-or-later

//! Minimal HTTP fetch abstraction, so the provider logic can be tested
//! offline against a fake implementation.

use std::future::Future;

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
