//! Responsibility: talks to the TONE3000 API v1 over HTTPS.
//!
//! The Secret Key goes only in the `Authorization` header of requests to
//! the API host; ureq drops it when a download redirects to storage.

use std::io::Write;
use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;

use super::api_client::{status_error, ApiError, Tone3000Api};
use super::api_enums::Tone3000Architecture;
use super::api_types::{Model, Page, Tone};
use super::api_url::{models_url, search_url, tone_url, SearchQuery};

/// Largest capture file accepted; real `.nam` / `.wav` files are a few MB.
const MAX_DOWNLOAD_BYTES: u64 = 256 * 1024 * 1024;

const USER_AGENT: &str = concat!("OpenRig/", env!("CARGO_PKG_VERSION"));

pub struct HttpTone3000Api {
    agent: ureq::Agent,
    key: String,
}

impl HttpTone3000Api {
    pub fn new(key: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(15)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_recv_body(Some(Duration::from_secs(300)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            key: key.to_string(),
        }
    }

    fn get(&self, url: &str) -> Result<ureq::http::Response<ureq::Body>, ApiError> {
        let response = self
            .agent
            .get(url)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("User-Agent", USER_AGENT)
            .call()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        let status = response.status().as_u16();
        if (200..300).contains(&status) {
            Ok(response)
        } else {
            Err(status_error(status))
        }
    }

    fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, ApiError> {
        let body = self
            .get(url)?
            .body_mut()
            .read_to_string()
            .map_err(|e| ApiError::Network(e.to_string()))?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse(e.to_string()))
    }
}

impl Tone3000Api for HttpTone3000Api {
    fn search(&self, query: &SearchQuery) -> Result<Page<Tone>, ApiError> {
        self.get_json(&search_url(query))
    }

    fn tone(&self, id: u64) -> Result<Tone, ApiError> {
        self.get_json(&tone_url(id))
    }

    fn models_page(
        &self,
        tone_id: u64,
        architecture: Option<Tone3000Architecture>,
        page: u32,
    ) -> Result<Page<Model>, ApiError> {
        self.get_json(&models_url(tone_id, architecture, page))
    }

    fn download(&self, url: &str, dest: &Path) -> Result<(), ApiError> {
        let mut response = self.get(url)?;
        let mut reader = response
            .body_mut()
            .with_config()
            .limit(MAX_DOWNLOAD_BYTES)
            .reader();
        let mut file = std::fs::File::create(dest).map_err(|e| ApiError::Io(e.to_string()))?;
        let copied = std::io::copy(&mut reader, &mut file).and_then(|_| file.flush());
        if let Err(e) = copied {
            drop(file);
            let _ = std::fs::remove_file(dest);
            return Err(ApiError::Network(e.to_string()));
        }
        Ok(())
    }
}
