use serde_json::Value;
use std::collections::HashSet;

fn candidates(metadata: &Value, kinds: &[&str]) -> Vec<String> {
    let Some(images) = metadata.get("keyImages").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    kinds.iter().flat_map(|kind| images.iter().filter(move |image| {
        image.get("type").and_then(Value::as_str) == Some(*kind)
    })).filter_map(|image| image.get("url").and_then(Value::as_str))
        .map(str::trim)
        .filter(|url| url.starts_with("https://") && seen.insert((*url).to_owned()))
        .map(str::to_owned).collect()
}

pub(crate) fn covers(metadata: &Value) -> Vec<String> {
    candidates(metadata, &["DieselGameBoxTall", "OfferImageTall", "DieselGameBox", "Thumbnail"])
}

pub(crate) fn heroes(metadata: &Value) -> Vec<String> {
    candidates(metadata, &["DieselStoreFrontWide", "OfferImageWide", "DieselGameBoxWide", "Featured", "DieselStoreFront", "Thumbnail"])
}
