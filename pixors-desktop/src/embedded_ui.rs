use std::borrow::Cow;
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../pixors-ui/dist/"]
struct UiAssets;

pub fn get(path: &str) -> Cow<'static, [u8]> {
    let path = if path.is_empty() || path == "/" {
        "index.html"
    } else {
        path
    };

    UiAssets::get(path)
        .map(|f| f.data)
        .or_else(|| UiAssets::get("index.html").map(|f| f.data))
        .unwrap_or_else(|| Cow::Borrowed(b"Not Found"))
}
