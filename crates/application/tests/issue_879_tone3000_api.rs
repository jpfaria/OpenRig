//! #879: the TONE3000 API v1 payloads parse into the app's types, and the
//! request URLs carry exactly the filters the API understands.

use std::path::PathBuf;

use application::tone3000::api_client::{status_error, ApiError};
use application::tone3000::api_types::{Model, Page, Tone};
use application::tone3000::api_url::{models_url, search_url, tone_url, SearchQuery};
use application::tone3000::{Tone3000Architecture, Tone3000Format, Tone3000Gear, Tone3000Sort};

fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/tone3000")
        .join(name);
    std::fs::read_to_string(path).expect("fixture")
}

#[test]
fn a_search_page_parses_into_tones() {
    let page: Page<Tone> = serde_json::from_str(&fixture("search_nam.json")).unwrap();
    assert_eq!(
        (page.page, page.page_size, page.total, page.total_pages),
        (1, 5, 47, 10)
    );
    let tone = &page.data[0];
    assert_eq!(tone.id, 66606);
    assert_eq!(tone.title, "REVV GENERATOR 120 (PURPLE) MKII");
    assert_eq!(tone.gear.as_deref(), Some("amp"));
    assert_eq!(tone.format.as_deref(), Some("nam"));
    assert_eq!(tone.a1_models_count, 10);
    assert_eq!(tone.a2_models_count, 20);
    assert_eq!(tone.images.len(), 1);
    assert_eq!(tone.makes[0].name, "REVV GENERATOR 120 MKII");
    assert!(tone.tags.iter().any(|t| t.name == "high gain amp"));
    let user = tone.user.as_ref().unwrap();
    assert_eq!(user.username, "deathblossomaudio");
    assert_eq!(user.display_name.as_deref(), Some("Deathblossomaudio"));
    assert_eq!(
        tone.url.as_deref(),
        Some("https://www.tone3000.com/tones/revv-generator-120-purple-mkii-66606")
    );
    assert_eq!(tone.downloads_count, 3239);
}

#[test]
fn a_tone_without_images_or_display_name_parses() {
    let tone: Tone = serde_json::from_str(&fixture("tone_716.json")).unwrap();
    assert_eq!(tone.id, 716);
    assert!(tone.images.is_empty());
    assert_eq!(tone.makes.len(), 2);
    assert_eq!(tone.user.as_ref().unwrap().display_name, None);
}

#[test]
fn an_ir_search_page_parses() {
    let page: Page<Tone> = serde_json::from_str(&fixture("search_ir.json")).unwrap();
    let tone = &page.data[0];
    assert_eq!(tone.id, 67544);
    assert_eq!(tone.gear.as_deref(), Some("cab"));
    assert_eq!(tone.format.as_deref(), Some("ir"));
    assert!(tone.irs_count > 0);
}

#[test]
fn a_tone_with_only_an_id_and_a_title_parses() {
    let tone: Tone = serde_json::from_str(r#"{"id": 1, "title": "Bare"}"#).unwrap();
    assert_eq!(tone.title, "Bare");
    assert!(tone.makes.is_empty() && tone.tags.is_empty() && tone.user.is_none());
}

#[test]
fn model_rows_keep_their_nullable_fields() {
    let ir: Page<Model> = serde_json::from_str(&fixture("models_67544_ir.json")).unwrap();
    assert_eq!(ir.data.len(), 12);
    assert_eq!(ir.data[0].size, None);
    assert_eq!(ir.data[0].architecture_version, None);
    assert!(ir.data[0].model_url.as_deref().unwrap().ends_with(".wav"));

    let nam: Page<Model> = serde_json::from_str(&fixture("models_1071_a1.json")).unwrap();
    assert_eq!(nam.data[0].tone_id, 1071);
    assert_eq!(nam.data[0].size.as_deref(), Some("standard"));
    assert_eq!(nam.data[0].architecture_version.as_deref(), Some("1"));
}

#[test]
fn the_search_url_carries_every_filter() {
    let query = SearchQuery {
        query: "jcm 800".into(),
        page: 2,
        page_size: 25,
        format: Some(Tone3000Format::Nam),
        gear: Some(Tone3000Gear::FullRig),
        sort: Some(Tone3000Sort::DownloadsAllTime),
    };
    assert_eq!(
        search_url(&query),
        "https://www.tone3000.com/api/v1/tones/search?query=jcm%20800&page=2&page_size=25\
         &format=nam&gears=full-rig&sort=downloads-all-time"
    );
}

#[test]
fn the_search_url_leaves_unset_filters_out() {
    let query = SearchQuery {
        query: String::new(),
        page: 1,
        page_size: 25,
        format: None,
        gear: None,
        sort: None,
    };
    assert_eq!(
        search_url(&query),
        "https://www.tone3000.com/api/v1/tones/search?query=&page=1&page_size=25"
    );
}

#[test]
fn the_search_url_escapes_reserved_characters() {
    let query = SearchQuery {
        query: "a&b=c/d?".into(),
        page: 1,
        page_size: 25,
        format: None,
        gear: None,
        sort: None,
    };
    assert!(search_url(&query).contains("query=a%26b%3Dc%2Fd%3F&"));
}

#[test]
fn every_gear_and_sort_has_its_api_spelling() {
    let gears = [
        (Tone3000Gear::Amp, "amp"),
        (Tone3000Gear::FullRig, "full-rig"),
        (Tone3000Gear::AmpCab, "amp-cab"),
        (Tone3000Gear::Pedal, "pedal"),
        (Tone3000Gear::Outboard, "outboard"),
        (Tone3000Gear::Ir, "ir"),
        (Tone3000Gear::Cab, "cab"),
    ];
    for (gear, spelling) in gears {
        assert_eq!(gear.as_api_str(), spelling);
    }
    let sorts = [
        (Tone3000Sort::BestMatch, "best-match"),
        (Tone3000Sort::Newest, "newest"),
        (Tone3000Sort::Oldest, "oldest"),
        (Tone3000Sort::Trending, "trending"),
        (Tone3000Sort::DownloadsAllTime, "downloads-all-time"),
    ];
    for (sort, spelling) in sorts {
        assert_eq!(sort.as_api_str(), spelling);
    }
}

#[test]
fn the_tone_url_names_one_tone() {
    assert_eq!(tone_url(716), "https://www.tone3000.com/api/v1/tones/716");
}

#[test]
fn the_models_url_filters_by_architecture() {
    assert_eq!(
        models_url(66606, Some(Tone3000Architecture::A2), 1),
        "https://www.tone3000.com/api/v1/models?tone_id=66606&page=1&page_size=100&architecture=2"
    );
    assert_eq!(
        models_url(66606, Some(Tone3000Architecture::A1), 3),
        "https://www.tone3000.com/api/v1/models?tone_id=66606&page=3&page_size=100&architecture=1"
    );
    assert_eq!(
        models_url(67544, None, 1),
        "https://www.tone3000.com/api/v1/models?tone_id=67544&page=1&page_size=100"
    );
}

#[test]
fn http_statuses_map_to_errors_the_user_can_act_on() {
    assert_eq!(status_error(401), ApiError::InvalidKey);
    assert_eq!(status_error(403), ApiError::InvalidKey);
    assert_eq!(status_error(429), ApiError::RateLimited);
    assert_eq!(status_error(404), ApiError::Http(404));
    assert_eq!(status_error(500), ApiError::Http(500));
}
