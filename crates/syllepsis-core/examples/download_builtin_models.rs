#[cfg(feature = "onnx")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::path::PathBuf;

    use syllepsis_core::onnx::{
        builtin, builtin_manifests, download_missing, inspect_model_cache, HttpModelFetcher,
        ModelCache,
    };

    let mut args = std::env::args().skip(1);
    let Some(cache_root) = args.next() else {
        print_usage();
        std::process::exit(2);
    };
    if cache_root == "--help" || cache_root == "-h" {
        print_usage();
        return Ok(());
    }

    let requested_model_ids: Vec<String> = args.collect();
    let manifests = if requested_model_ids.is_empty() {
        builtin_manifests()
    } else {
        requested_model_ids
            .iter()
            .map(|model_id| {
                builtin(model_id).ok_or_else(|| format!("unknown built-in model id: {model_id}"))
            })
            .collect::<Result<Vec<_>, _>>()?
    };

    let cache_root = PathBuf::from(cache_root);
    let cache = ModelCache::new(&cache_root);
    let fetcher = HttpModelFetcher::new()?;

    for manifest in &manifests {
        println!("== {} ({}) ==", manifest.display_name, manifest.id);
        let before = inspect_model_cache(&cache, manifest, false)?;
        if before.cached {
            println!("cache already has every expected file");
        }

        let report = download_missing(&cache, manifest, &fetcher)?;
        if report.is_empty() {
            println!("downloaded 0 files");
        } else {
            for (file_name, integrity) in report {
                println!("downloaded {file_name}: {integrity:?}");
            }
        }

        let after = inspect_model_cache(&cache, manifest, true)?;
        if !after.loadable {
            return Err(format!("{} is still not loadable after download", manifest.id).into());
        }
        println!("verified {} files", after.files.len());
    }

    println!();
    println!("Run live ONNX tests with:");
    println!(
        "SYLLEPSIS_MODEL_CACHE={} cargo test -p syllepsis-core --features onnx --test onnx_live -- --ignored --nocapture",
        cache_root.display()
    );

    Ok(())
}

#[cfg(feature = "onnx")]
fn print_usage() {
    eprintln!(
        "usage: cargo run -p syllepsis-core --features onnx --example download_builtin_models -- <cache-dir> [model-id ...]"
    );
    eprintln!("known model IDs:");
    for manifest in syllepsis_core::onnx::builtin_manifests() {
        eprintln!("  {}", manifest.id);
    }
}

#[cfg(not(feature = "onnx"))]
fn main() {
    eprintln!("this example requires the syllepsis-core `onnx` feature");
    eprintln!(
        "run: cargo run -p syllepsis-core --features onnx --example download_builtin_models -- <cache-dir>"
    );
    std::process::exit(2);
}
