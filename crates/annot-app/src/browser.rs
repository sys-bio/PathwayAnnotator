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
