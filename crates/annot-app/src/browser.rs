//! What the app needs from the browser: fetching the knowledge pack,
//! reading a chosen file, and saving one.

use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

fn js_error(v: JsValue) -> String {
    v.as_string().unwrap_or_else(|| format!("{v:?}"))
}

/// Fetches the knowledge pack, served next to the app.
pub async fn fetch_pack(url: &str) -> Result<kpack::Pack, String> {
    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(js_error)?
        .dyn_into()
        .map_err(js_error)?;
    if !resp.ok() {
        return Err(format!("the server answered {}", resp.status()));
    }
    let buf = JsFuture::from(resp.array_buffer().map_err(js_error)?)
        .await
        .map_err(js_error)?;
    let bytes = js_sys::Uint8Array::new(&buf).to_vec();
    kpack::decode(&bytes).map_err(|e| e.to_string())
}

/// Fetches a URL's text: an answer from another database.
pub async fn fetch_text(url: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|_| "the service could not be reached (offline?)".to_string())?
        .dyn_into()
        .map_err(js_error)?;
    if !resp.ok() {
        return Err(format!("the service answered {}", resp.status()));
    }
    let text = JsFuture::from(resp.text().map_err(js_error)?).await.map_err(js_error)?;
    text.as_string().ok_or_else(|| "the answer is not text".into())
}

/// The time now as W3CDTF, in UTC to the second: "2026-11-14T22:50:12Z".
pub fn now() -> String {
    let iso = String::from(js_sys::Date::new_0().to_iso_string());
    match iso.split_once('.') {
        Some((head, _)) => format!("{head}Z"),
        None => iso,
    }
}

/// Where the names of entries accepted from other databases are kept, in
/// this browser only.
const NAMES_KEY: &str = "pathwayannotator.online-names";

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

/// Names of entries accepted from other databases in earlier sessions:
/// (URI, plain name, name with database and id). Empty if the browser
/// keeps nothing (private window, storage blocked).
pub fn remembered_names() -> Vec<(String, String, String)> {
    let Some(text) = local_storage().and_then(|s| s.get_item(NAMES_KEY).ok().flatten()) else {
        return Vec::new();
    };
    let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    map.into_iter()
        .filter_map(|(uri, v)| {
            let a = v.as_array()?;
            Some((uri, a.first()?.as_str()?.to_string(), a.get(1)?.as_str()?.to_string()))
        })
        .collect()
}

/// Keeps the name of an accepted entry for later sessions. Failing to
/// keep it (storage blocked or full) only means it is not remembered.
pub fn remember_name(uri: &str, name: &str, label: &str) {
    let Some(store) = local_storage() else { return };
    let mut map = match store.get_item(NAMES_KEY).ok().flatten().map(|t| serde_json::from_str(&t)) {
        Some(Ok(serde_json::Value::Object(m))) => m,
        _ => serde_json::Map::new(),
    };
    map.insert(uri.to_string(), serde_json::json!([name, label]));
    let _ = store.set_item(NAMES_KEY, &serde_json::Value::Object(map).to_string());
}

/// Asks the user a yes/no question with the browser's own dialog.
/// True if they agree (or there is no window to ask in).
pub fn confirm(question: &str) -> bool {
    web_sys::window().is_none_or(|w| w.confirm_with_message(question).unwrap_or(true))
}

/// The text of a file the user chose or dropped.
pub async fn read_file(file: &web_sys::File) -> Result<String, String> {
    let text = JsFuture::from(file.text()).await.map_err(js_error)?;
    text.as_string().ok_or_else(|| "the file is not text".into())
}

/// Offers `text` to the user as a download named `name`.
pub fn save_file(name: &str, text: &str) -> Result<(), String> {
    let parts = js_sys::Array::new();
    parts.push(&JsValue::from_str(text));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type("application/xml");
    let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts).map_err(js_error)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js_error)?;
    let document = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(js_error)?
        .dyn_into()
        .map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url).map_err(js_error)?;
    Ok(())
}
