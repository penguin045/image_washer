use std::env;
use std::fs;
use std::path::Path;

use image_washer::{is_supported_image, parse_args, print_help, wash_image_bytes_from_name};

#[derive(Debug)]
struct Summary {
    washed: usize,
    skipped: usize,
    failed: usize,
}

fn main() {
    let exit_code = match run() {
        Ok(Some(summary)) => {
            println!(
                "washed: {}  skipped: {}  failed: {}",
                summary.washed, summary.skipped, summary.failed
            );
            if summary.failed > 0 {
                1
            } else {
                0
            }
        }
        Ok(None) => 0,
        Err(err) => {
            eprintln!("{err}");
            2
        }
    };

    std::process::exit(exit_code);
}

fn run() -> Result<Option<Summary>, String> {
    let config = match parse_args(env::args().skip(1)) {
        Ok(config) => config,
        Err(err) if err == "help" => {
            print_help();
            return Ok(None);
        }
        Err(err) => return Err(err),
    };

    if !config.input_dir.exists() {
        return Err(format!(
            "input directory does not exist: {}",
            config.input_dir.display()
        ));
    }
    if !config.input_dir.is_dir() {
        return Err(format!(
            "input path is not a directory: {}",
            config.input_dir.display()
        ));
    }
    if config.input_dir == config.output_dir {
        return Err("input and output directories must be different".to_string());
    }

    fs::create_dir_all(&config.output_dir).map_err(|err| {
        format!(
            "failed to create output directory {}: {err}",
            config.output_dir.display()
        )
    })?;

    let mut summary = Summary {
        washed: 0,
        skipped: 0,
        failed: 0,
    };

    visit_dir(
        &config.input_dir,
        &config.input_dir,
        &config.output_dir,
        &mut summary,
    )?;

    Ok(Some(summary))
}

fn visit_dir(
    current_dir: &Path,
    input_root: &Path,
    output_root: &Path,
    summary: &mut Summary,
) -> Result<(), String> {
    let entries = fs::read_dir(current_dir)
        .map_err(|err| format!("failed to read directory {}: {err}", current_dir.display()))?;

    for entry in entries {
        let entry = entry
            .map_err(|err| format!("failed to read entry in {}: {err}", current_dir.display()))?;
        let path = entry.path();

        if path.is_dir() {
            visit_dir(&path, input_root, output_root, summary)?;
            continue;
        }

        if !is_supported_image(&path) {
            summary.skipped += 1;
            continue;
        }

        let relative = path.strip_prefix(input_root).map_err(|err| {
            format!(
                "failed to build relative path for {}: {err}",
                path.display()
            )
        })?;
        let output_path = output_root.join(relative);

        match wash_image(&path, &output_path) {
            Ok(()) => {
                summary.washed += 1;
                println!("washed: {} -> {}", path.display(), output_path.display());
            }
            Err(err) => {
                summary.failed += 1;
                eprintln!("failed: {} ({err})", path.display());
            }
        }
    }

    Ok(())
}

fn wash_image(input_path: &Path, output_path: &Path) -> Result<(), String> {
    let bytes = fs::read(input_path)
        .map_err(|err| format!("failed to read image {}: {err}", input_path.display()))?;
    let file_name = input_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("invalid file name: {}", input_path.display()))?;
    let washed = wash_image_bytes_from_name(&bytes, file_name)?;

    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create output directory {}: {err}",
                parent.display()
            )
        })?;
    }

    fs::write(output_path, washed)
        .map_err(|err| format!("failed to write {}: {err}", output_path.display()))?;

    Ok(())
}
