use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use exif::{In, Reader as ExifReader, Tag, Value};
use image::imageops;
use image::{DynamicImage, ImageFormat};

pub const DEFAULT_INPUT_DIR: &str = "input";
pub const DEFAULT_OUTPUT_DIR: &str = "output";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub input_dir: PathBuf,
    pub output_dir: PathBuf,
}

pub fn parse_args<I>(args: I) -> Result<Config, String>
where
    I: IntoIterator,
    I::Item: Into<std::ffi::OsString>,
{
    let mut input_dir = PathBuf::from(DEFAULT_INPUT_DIR);
    let mut output_dir = PathBuf::from(DEFAULT_OUTPUT_DIR);
    let mut args = args.into_iter().map(Into::into);

    while let Some(arg) = args.next() {
        match arg.to_string_lossy().as_ref() {
            "-i" | "--input-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --input-dir".to_string())?;
                input_dir = PathBuf::from(value);
            }
            "-o" | "--output-dir" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --output-dir".to_string())?;
                output_dir = PathBuf::from(value);
            }
            "-h" | "--help" => {
                return Err("help".to_string());
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
    }

    Ok(Config {
        input_dir,
        output_dir,
    })
}

pub fn print_help() {
    println!("image_washer");
    println!("Strip image metadata by decoding pixels and re-encoding clean files.");
    println!();
    println!("USAGE:");
    println!("  cargo run -- [--input-dir DIR] [--output-dir DIR]");
    println!("  ./target/release/image_washer [--input-dir DIR] [--output-dir DIR]");
    println!();
    println!("DEFAULTS:");
    println!("  --input-dir   ./{}", DEFAULT_INPUT_DIR);
    println!("  --output-dir  ./{}", DEFAULT_OUTPUT_DIR);
}

pub fn is_supported_image(path: &Path) -> bool {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
    {
        Some(ext) => is_supported_extension(&ext),
        None => false,
    }
}

pub fn is_supported_extension(extension: &str) -> bool {
    matches!(
        extension,
        "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "bmp" | "gif"
    )
}

pub fn infer_format_from_name(name: &str) -> Result<ImageFormat, String> {
    let extension = Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .ok_or_else(|| format!("unsupported image extension: {name}"))?;

    match extension.as_str() {
        "jpg" | "jpeg" => Ok(ImageFormat::Jpeg),
        "png" => Ok(ImageFormat::Png),
        "webp" => Ok(ImageFormat::WebP),
        "tif" | "tiff" => Ok(ImageFormat::Tiff),
        "bmp" => Ok(ImageFormat::Bmp),
        "gif" => Ok(ImageFormat::Gif),
        _ => Err(format!("unsupported image extension: {name}")),
    }
}

pub fn wash_image_bytes(bytes: &[u8], format: ImageFormat) -> Result<Vec<u8>, String> {
    let mut image = image::load_from_memory_with_format(bytes, format)
        .map_err(|err| format!("failed to decode image: {err}"))?;

    if matches!(format, ImageFormat::Gif) && looks_animated_gif(bytes) {
        return Err("animated GIF is not supported".to_string());
    }

    image = apply_orientation(image, bytes)?;

    let mut output = Cursor::new(Vec::new());
    image
        .write_to(&mut output, format)
        .map_err(|err| format!("failed to encode image: {err}"))?;

    Ok(output.into_inner())
}

pub fn wash_image_bytes_from_name(bytes: &[u8], file_name: &str) -> Result<Vec<u8>, String> {
    let format = infer_format_from_name(file_name)?;
    wash_image_bytes(bytes, format)
}

fn looks_animated_gif(bytes: &[u8]) -> bool {
    bytes.windows(11).any(|window| window == b"NETSCAPE2.0")
}

fn apply_orientation(image: DynamicImage, bytes: &[u8]) -> Result<DynamicImage, String> {
    let cursor = Cursor::new(bytes);
    let mut reader = BufReader::new(cursor);
    let exif = match ExifReader::new().read_from_container(&mut reader) {
        Ok(exif) => exif,
        Err(_) => return Ok(image),
    };

    let field = match exif.get_field(Tag::Orientation, In::PRIMARY) {
        Some(field) => field,
        None => return Ok(image),
    };

    let orientation = match &field.value {
        Value::Short(values) if !values.is_empty() => values[0],
        _ => return Ok(image),
    };

    let applied = match orientation {
        1 => image,
        2 => DynamicImage::ImageRgba8(imageops::flip_horizontal(&image.to_rgba8())),
        3 => DynamicImage::ImageRgba8(imageops::rotate180(&image.to_rgba8())),
        4 => DynamicImage::ImageRgba8(imageops::flip_vertical(&image.to_rgba8())),
        5 => DynamicImage::ImageRgba8(imageops::rotate90(&imageops::flip_horizontal(
            &image.to_rgba8(),
        ))),
        6 => DynamicImage::ImageRgba8(imageops::rotate90(&image.to_rgba8())),
        7 => DynamicImage::ImageRgba8(imageops::rotate270(&imageops::flip_horizontal(
            &image.to_rgba8(),
        ))),
        8 => DynamicImage::ImageRgba8(imageops::rotate270(&image.to_rgba8())),
        _ => image,
    };

    Ok(applied)
}

#[cfg(target_arch = "wasm32")]
mod wasm_exports {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(js_name = washImage)]
    pub fn wash_image(bytes: &[u8], file_name: &str) -> Result<Vec<u8>, JsValue> {
        crate::wash_image_bytes_from_name(bytes, file_name)
            .map_err(|message| JsValue::from_str(&message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::RgbImage;

    #[test]
    fn parse_args_uses_default_directories() {
        let config = parse_args(Vec::<String>::new()).expect("default args should parse");

        assert_eq!(config.input_dir, PathBuf::from(DEFAULT_INPUT_DIR));
        assert_eq!(config.output_dir, PathBuf::from(DEFAULT_OUTPUT_DIR));
    }

    #[test]
    fn parse_args_accepts_custom_directories() {
        let config = parse_args(vec!["--input-dir", "images", "--output-dir", "washed"])
            .expect("custom args should parse");

        assert_eq!(config.input_dir, PathBuf::from("images"));
        assert_eq!(config.output_dir, PathBuf::from("washed"));
    }

    #[test]
    fn supported_extensions_are_case_insensitive() {
        assert!(is_supported_image(Path::new("photo.JPG")));
        assert!(is_supported_image(Path::new("photo.webp")));
        assert!(!is_supported_image(Path::new("notes.txt")));
    }

    #[test]
    fn wash_image_removes_jpeg_exif() {
        let image = RgbImage::from_pixel(4, 4, image::Rgb([255, 0, 0]));
        let mut buffer = Vec::new();
        let mut encoder = JpegEncoder::new_with_quality(&mut buffer, 90);
        let mut exif = vec![
            0x45, 0x78, 0x69, 0x66, 0x00, 0x00, 0x4d, 0x4d, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x08,
            0x00, 0x00,
        ];

        encoder
            .encode_image(&DynamicImage::ImageRgb8(image))
            .expect("source jpeg should encode");

        let mut source = Vec::new();
        source.extend_from_slice(&buffer[..2]);
        source.extend_from_slice(&[0xff, 0xe1]);
        source.extend_from_slice(&[
            ((exif.len() + 2) >> 8) as u8,
            ((exif.len() + 2) & 0xff) as u8,
        ]);
        source.append(&mut exif);
        source.extend_from_slice(&buffer[2..]);

        let washed =
            wash_image_bytes_from_name(&source, "test.jpg").expect("washed jpeg should encode");

        assert!(washed.windows(6).all(|window| window != b"Exif\0\0"));
    }
}
