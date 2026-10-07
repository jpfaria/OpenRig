//! Responsibility: builds the TONE3000 API v1 request URLs.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};

use super::api_enums::{Tone3000Architecture, Tone3000Format, Tone3000Gear, Tone3000Sort};

pub const API_BASE: &str = "https://www.tone3000.com/api/v1";

/// Rows per `GET /models` page: the API maximum, so most tones need one call.
pub const MODELS_PAGE_SIZE: u32 = 100;

/// Everything but RFC 3986 unreserved characters is escaped.
const QUERY_VALUE: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// One `GET /tones/search` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub page: u32,
    pub page_size: u32,
    pub format: Option<Tone3000Format>,
    pub gear: Option<Tone3000Gear>,
    pub sort: Option<Tone3000Sort>,
}

pub fn search_url(query: &SearchQuery) -> String {
    let mut url = format!(
        "{API_BASE}/tones/search?query={}&page={}&page_size={}",
        utf8_percent_encode(&query.query, QUERY_VALUE),
        query.page,
        query.page_size
    );
    if let Some(format) = query.format {
        url.push_str("&format=");
        url.push_str(format.as_api_str());
    }
    if let Some(gear) = query.gear {
        url.push_str("&gears=");
        url.push_str(gear.as_api_str());
    }
    if let Some(sort) = query.sort {
        url.push_str("&sort=");
        url.push_str(sort.as_api_str());
    }
    url
}

pub fn tone_url(tone_id: u64) -> String {
    format!("{API_BASE}/tones/{tone_id}")
}

/// `architecture` is left out for IR tones, whose rows carry none.
pub fn models_url(tone_id: u64, architecture: Option<Tone3000Architecture>, page: u32) -> String {
    let mut url =
        format!("{API_BASE}/models?tone_id={tone_id}&page={page}&page_size={MODELS_PAGE_SIZE}");
    if let Some(architecture) = architecture {
        url.push_str("&architecture=");
        url.push_str(architecture.as_api_str());
    }
    url
}
