use anyhow::anyhow;
use gpui::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;
use std::borrow::Cow;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "fonts/*.ttf"]
#[include = "icons/*.png"]
pub struct EmbeddedAssets;

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(file) = EmbeddedAssets::get(path) {
            return Ok(Some(file.data));
        }
        // Fallback to gpui-component built-in icon assets if requested
        if let Ok(Some(data)) = gpui_kit_assets::Assets.load(path) {
            return Ok(Some(data));
        }
        Err(anyhow!("Asset not found: {path}"))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(EmbeddedAssets::iter()
            .filter(|p| p.starts_with(path))
            .map(|p| SharedString::from(p.into_owned()))
            .collect())
    }
}

pub fn load_embedded_fonts(cx: &gpui::App) {
    let font_paths = [
        "fonts/X-VF.ttf",
        "fonts/XMono-VF.ttf",
        "fonts/cursor-icons-16.ttf",
    ];
    let mut fonts = Vec::new();
    for path in font_paths {
        if let Some(file) = EmbeddedAssets::get(path) {
            fonts.push(file.data);
        }
    }
    if !fonts.is_empty() {
        let _ = cx.text_system().add_fonts(fonts);
    }
}
