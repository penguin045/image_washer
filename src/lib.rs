use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use exif::{In, Reader as ExifReader, Tag, Value};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{
    CompressionType as PngCompressionType, FilterType as PngFilterType, PngEncoder,
};
use image::imageops;
use image::{DynamicImage, GenericImageView, ImageEncoder, ImageFormat};

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
    if matches!(format, ImageFormat::Png) {
        return strip_png_metadata_chunks(bytes).or_else(|_| reencode_image_bytes(bytes, format));
    }

    reencode_image_bytes(bytes, format)
}

fn reencode_image_bytes(bytes: &[u8], format: ImageFormat) -> Result<Vec<u8>, String> {
    let mut image = image::load_from_memory_with_format(bytes, format)
        .map_err(|err| format!("failed to decode image: {err}"))?;

    if matches!(format, ImageFormat::Gif) && looks_animated_gif(bytes) {
        return Err("animated GIF is not supported".to_string());
    }

    image = apply_orientation(image, bytes)?;

    encode_clean_image(&image, format, bytes.len())
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

fn encode_clean_image(
    image: &DynamicImage,
    format: ImageFormat,
    source_len: usize,
) -> Result<Vec<u8>, String> {
    match format {
        ImageFormat::Jpeg => encode_jpeg_size_conscious(image, source_len),
        ImageFormat::Png => encode_png_best(image),
        _ => {
            let mut output = Cursor::new(Vec::new());
            image
                .write_to(&mut output, format)
                .map_err(|err| format!("failed to encode image: {err}"))?;
            Ok(output.into_inner())
        }
    }
}

fn encode_png_best(image: &DynamicImage) -> Result<Vec<u8>, String> {
    let (width, height) = image.dimensions();
    let mut output = Cursor::new(Vec::new());

    PngEncoder::new_with_quality(
        &mut output,
        PngCompressionType::Best,
        PngFilterType::Adaptive,
    )
    .write_image(image.as_bytes(), width, height, image.color())
    .map_err(|err| format!("failed to encode image: {err}"))?;

    Ok(output.into_inner())
}

fn encode_jpeg_size_conscious(image: &DynamicImage, source_len: usize) -> Result<Vec<u8>, String> {
    let rgb = image.to_rgb8();
    let (width, height) = rgb.dimensions();
    let mut smallest: Option<Vec<u8>> = None;

    for quality in [75_u8, 68_u8, 60_u8] {
        let mut output = Cursor::new(Vec::new());
        JpegEncoder::new_with_quality(&mut output, quality)
            .write_image(rgb.as_raw(), width, height, image::ColorType::Rgb8)
            .map_err(|err| format!("failed to encode image: {err}"))?;

        let encoded = output.into_inner();
        if encoded.len() <= source_len {
            return Ok(encoded);
        }

        match &smallest {
            Some(current) if current.len() <= encoded.len() => {}
            _ => smallest = Some(encoded),
        }
    }

    smallest.ok_or_else(|| "failed to encode image".to_string())
}

fn strip_png_metadata_chunks(bytes: &[u8]) -> Result<Vec<u8>, String> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err("not a PNG file".to_string());
    }

    let mut offset = PNG_SIGNATURE.len();
    let mut output = Vec::with_capacity(bytes.len());
    output.extend_from_slice(PNG_SIGNATURE);

    while offset + 12 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_start = offset;
        let chunk_type_start = offset + 4;
        let data_start = offset + 8;
        let crc_start = data_start + length;
        let next_offset = crc_start + 4;

        if next_offset > bytes.len() {
            return Err("truncated PNG chunk".to_string());
        }

        let chunk_type = &bytes[chunk_type_start..data_start];
        if chunk_type == b"acTL" || chunk_type == b"fcTL" || chunk_type == b"fdAT" {
            return Err("animated PNG is not supported".to_string());
        }

        if should_keep_png_chunk(chunk_type) {
            output.extend_from_slice(&bytes[chunk_start..next_offset]);
        }

        offset = next_offset;

        if chunk_type == b"IEND" {
            return Ok(output);
        }
    }

    Err("missing PNG IEND chunk".to_string())
}

fn should_keep_png_chunk(chunk_type: &[u8]) -> bool {
    matches!(chunk_type, b"IHDR" | b"PLTE" | b"IDAT" | b"IEND" | b"tRNS")
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
    use image::codecs::png::PngEncoder;
    use image::ImageEncoder;
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

    #[test]
    fn wash_image_removes_novelai_png_text_chunks() {
        let image = RgbImage::from_pixel(4, 4, image::Rgb([0, 128, 255]));
        let mut source = Vec::new();
        PngEncoder::new(&mut source)
            .write_image(image.as_raw(), 4, 4, image::ColorType::Rgb8)
            .expect("source png should encode");

        let source = insert_png_chunk(&source, b"tEXt", b"Description\x00masterpiece, 1girl");
        let source = insert_png_chunk(&source, b"iTXt", b"Comment\x00\x00\x00\x00\x00NovelAI tags");
        let source = insert_png_chunk(&source, b"eXIf", b"Exif\x00\x00NovelAI");

        let washed =
            wash_image_bytes_from_name(&source, "novelai.png").expect("washed png should encode");

        assert!(has_png_chunk(&source, b"tEXt"));
        assert!(has_png_chunk(&source, b"iTXt"));
        assert!(has_png_chunk(&source, b"eXIf"));
        assert!(!has_png_chunk(&washed, b"tEXt"));
        assert!(!has_png_chunk(&washed, b"iTXt"));
        assert!(!has_png_chunk(&washed, b"eXIf"));
        assert!(has_png_chunk(&washed, b"IHDR"));
        assert!(has_png_chunk(&washed, b"IDAT"));
        assert!(has_png_chunk(&washed, b"IEND"));
    }

    fn insert_png_chunk(source: &[u8], chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let ihdr_end = 8 + 4 + 4 + 13 + 4;
        let mut chunk = Vec::new();
        chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
        chunk.extend_from_slice(chunk_type);
        chunk.extend_from_slice(data);
        chunk.extend_from_slice(&crc32(chunk_type, data).to_be_bytes());

        let mut output = Vec::new();
        output.extend_from_slice(&source[..ihdr_end]);
        output.extend_from_slice(&chunk);
        output.extend_from_slice(&source[ihdr_end..]);
        output
    }

    fn has_png_chunk(source: &[u8], target: &[u8; 4]) -> bool {
        let mut offset = 8;
        while offset + 12 <= source.len() {
            let length = u32::from_be_bytes([
                source[offset],
                source[offset + 1],
                source[offset + 2],
                source[offset + 3],
            ]) as usize;
            let chunk_type = &source[offset + 4..offset + 8];
            if chunk_type == target {
                return true;
            }
            offset += 12 + length;
        }
        false
    }

    fn crc32(chunk_type: &[u8; 4], data: &[u8]) -> u32 {
        let mut crc = 0xFFFF_FFFF_u32;

        for byte in chunk_type.iter().chain(data.iter()) {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0_u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
            }
        }

        !crc
    }
}
