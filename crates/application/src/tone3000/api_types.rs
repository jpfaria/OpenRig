//! Responsibility: mirrors the TONE3000 API v1 response payloads.
//!
//! Only the fields the browser and the installer read are kept; everything
//! else in a response is ignored. Lists the API sends as `null` read as
//! empty, so a sparse tone never fails to parse.

use serde::{Deserialize, Deserializer, Serialize};

/// One page of a paginated endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    pub data: Vec<T>,
    pub page: u32,
    pub page_size: u32,
    pub total: u32,
    pub total_pages: u32,
}

/// A published tone: one rig, pedal or cab with its captures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tone {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub tags: Vec<Tag>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub makes: Vec<Make>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub images: Vec<String>,
    #[serde(default)]
    pub user: Option<User>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub gear: Option<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub models_count: u32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub a1_models_count: u32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub a2_models_count: u32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub irs_count: u32,
    #[serde(default, deserialize_with = "null_as_default")]
    pub downloads_count: u64,
    #[serde(default, deserialize_with = "null_as_default")]
    pub favorites_count: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Make {
    pub name: String,
}

/// The tone's author.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// One downloadable capture row of `GET /models`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model {
    pub id: u64,
    pub tone_id: u64,
    pub name: String,
    #[serde(default)]
    pub model_url: Option<String>,
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default, deserialize_with = "string_or_number")]
    pub architecture_version: Option<String>,
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn string_or_number<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(
        match Option::<serde_json::Value>::deserialize(deserializer)? {
            Some(serde_json::Value::String(s)) => Some(s),
            Some(serde_json::Value::Number(n)) => Some(n.to_string()),
            _ => None,
        },
    )
}
