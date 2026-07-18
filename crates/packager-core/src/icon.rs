use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use image::{imageops, DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;

const MAX_ICON_BYTES: usize = 10 * 1024 * 1024;
const MAX_ICON_DIMENSION: u32 = 8192;
const OUTPUT_SIZE: u32 = 1024;

pub(crate) fn normalize_to_png(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.is_empty() || bytes.len() > MAX_ICON_BYTES {
        return Err("App icon must be a non-empty image smaller than 10 MB".into());
    }
    let decoded = image::load_from_memory(bytes)
        .map_err(|error| format!("Cannot read app icon image: {error}"))?;
    if decoded.width() == 0
        || decoded.height() == 0
        || decoded.width() > MAX_ICON_DIMENSION
        || decoded.height() > MAX_ICON_DIMENSION
    {
        return Err("App icon dimensions must be between 1 and 8192 pixels".into());
    }

    let scale = (OUTPUT_SIZE as f64 / decoded.width() as f64)
        .min(OUTPUT_SIZE as f64 / decoded.height() as f64);
    let width = (decoded.width() as f64 * scale).round().max(1.0) as u32;
    let height = (decoded.height() as f64 * scale).round().max(1.0) as u32;
    let resized = imageops::resize(
        &decoded.to_rgba8(),
        width,
        height,
        imageops::FilterType::Lanczos3,
    );
    let mut canvas = RgbaImage::new(OUTPUT_SIZE, OUTPUT_SIZE);
    imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((OUTPUT_SIZE - width) / 2),
        i64::from((OUTPUT_SIZE - height) / 2),
    );

    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(canvas)
        .write_to(&mut output, ImageFormat::Png)
        .map_err(|error| format!("Cannot prepare app icon: {error}"))?;
    Ok(output.into_inner())
}

pub(crate) fn decode_data_url(data: &str) -> Result<Vec<u8>, String> {
    let (_, encoded) = data
        .split_once(',')
        .filter(|(header, _)| header.starts_with("data:image/") && header.ends_with(";base64"))
        .ok_or("Custom app icon must be a supported image")?;
    if encoded.len() > 14 * 1024 * 1024 {
        return Err("Custom app icon must be smaller than 10 MB".into());
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| "Custom app icon contains invalid image data")?;
    normalize_to_png(&bytes)
}

pub(crate) fn data_url(bytes: &[u8]) -> Result<String, String> {
    let png = normalize_to_png(bytes)?;
    Ok(format!("data:image/png;base64,{}", BASE64.encode(png)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_a_rectangular_icon_to_a_square_png() {
        let mut source = Cursor::new(Vec::new());
        DynamicImage::new_rgba8(40, 20)
            .write_to(&mut source, ImageFormat::Png)
            .expect("test icon should encode");
        let normalized = normalize_to_png(source.get_ref()).expect("icon should normalize");
        let decoded = image::load_from_memory(&normalized).expect("normalized icon should decode");
        assert_eq!((decoded.width(), decoded.height()), (1024, 1024));
    }

    #[test]
    fn validates_and_decodes_image_data_urls() {
        let mut source = Cursor::new(Vec::new());
        DynamicImage::new_rgba8(24, 24)
            .write_to(&mut source, ImageFormat::Png)
            .expect("test icon should encode");
        let encoded = format!(
            "data:image/png;base64,{}",
            BASE64.encode(source.into_inner())
        );
        let normalized = decode_data_url(&encoded).expect("data URL should decode");
        assert!(data_url(&normalized)
            .expect("normalized icon should encode")
            .starts_with("data:image/png;base64,"));
        assert!(decode_data_url("data:text/plain;base64,Zm9v").is_err());
    }
}
