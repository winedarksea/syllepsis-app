//! Validated image ingestion shared by inline Markdown images, Picture/Drawing objects, and worlds.

use std::path::{Path, PathBuf};

use image::ImageReader;
use serde::{Deserialize, Serialize};

use crate::app::NoteDto;
use crate::error::{CoreError, CoreResult};
use crate::model::{AssetMetadata, Note, ObjectType};
use crate::storage::Book;
use crate::sync::{assign_asset_uuid, AssetRegistry};

const ASSETS_DIRECTORY: &str = "assets";
const SUPPORTED_RASTER_EXTENSIONS: &[&str] = &["png", "jpg", "gif", "webp"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct InspectedImage {
    object_type: ObjectType,
    extension: &'static str,
    media_type: &'static str,
    dimensions: (u32, u32),
}

type TrackedAssetInspection = (ObjectType, (u32, u32), String);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportedAsset {
    pub uuid: String,
    pub relative_path: String,
    pub media_type: String,
    pub intrinsic_dimensions: (u32, u32),
    pub original_filename: String,
    pub object_type: ObjectType,
}

/// Import and validate an asset without creating a note. Used by inline Markdown insertion.
pub fn import_tracked_asset(book: &Book, source_path: &str) -> CoreResult<ImportedAsset> {
    let source = Path::new(source_path);
    let inspected = inspect_image(source)?;
    let original_filename = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CoreError::parse("image", "source filename is not valid UTF-8"))?
        .to_string();

    let assets_directory = book.root.join(ASSETS_DIRECTORY);
    std::fs::create_dir_all(&assets_directory)?;
    let file_stem = ulid::Ulid::new().to_string().to_lowercase();
    let file_name = format!("{file_stem}.{}", inspected.extension);
    let relative_path = format!("{ASSETS_DIRECTORY}/{file_name}");
    let final_path = assets_directory.join(&file_name);
    let temporary_path = assets_directory.join(format!(".{file_name}.importing"));

    std::fs::copy(source, &temporary_path)?;
    if let Err(error) = std::fs::rename(&temporary_path, &final_path) {
        let _ = std::fs::remove_file(&temporary_path);
        return Err(error.into());
    }

    let uuid = match assign_asset_uuid(&book.root, &relative_path) {
        Ok(uuid) => uuid,
        Err(error) => {
            let _ = std::fs::remove_file(&final_path);
            return Err(error);
        }
    };

    Ok(ImportedAsset {
        uuid,
        relative_path,
        media_type: inspected.media_type.to_string(),
        intrinsic_dimensions: inspected.dimensions,
        original_filename,
        object_type: inspected.object_type,
    })
}

/// Import a first-class Picture or Drawing note. Failed note persistence rolls back the copied
/// asset and UUID sidecar so the book never retains a half-created object.
pub fn import_image_object(
    book: &Book,
    source_path: &str,
    requested_title: Option<&str>,
) -> CoreResult<NoteDto> {
    let imported = import_tracked_asset(book, source_path)?;
    let title = requested_title
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            Path::new(&imported.original_filename)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Image")
                .to_string()
        });

    let mut note = Note::new(
        imported.object_type,
        title,
        book.config.markdown.dialect_version.clone(),
    );
    note.asset = Some(AssetMetadata {
        uuid: imported.uuid.clone(),
        media_type: imported.media_type.clone(),
        intrinsic_dimensions: imported.intrinsic_dimensions,
        original_filename: imported.original_filename.clone(),
    });

    if let Err(error) = book.save_note(&note) {
        rollback_import(book, &imported.relative_path);
        return Err(error);
    }
    Ok(NoteDto::from_note(&note))
}

/// Resolve a tracked image UUID to bytes and media type for a self-contained frontend data URL.
pub fn asset_file(book: &Book, asset_uuid: &str) -> CoreResult<Option<(PathBuf, String)>> {
    let registry = AssetRegistry::scan(&book.root)?;
    let Some(relative_path) = registry.resolve(asset_uuid) else {
        return Ok(None);
    };
    let path = book.root.join(relative_path);
    let inspected = inspect_image(&path)?;
    Ok(Some((path, inspected.media_type.to_string())))
}

/// Re-inspect a tracked asset from disk rather than trusting editable note metadata.
pub fn inspect_tracked_asset(
    book: &Book,
    asset_uuid: &str,
) -> CoreResult<Option<TrackedAssetInspection>> {
    let registry = AssetRegistry::scan(&book.root)?;
    let Some(relative_path) = registry.resolve(asset_uuid) else {
        return Ok(None);
    };
    let inspected = inspect_image(&book.root.join(relative_path))?;
    Ok(Some((
        inspected.object_type,
        inspected.dimensions,
        inspected.media_type.to_string(),
    )))
}

fn rollback_import(book: &Book, relative_path: &str) {
    let asset_path = book.root.join(relative_path);
    let sidecar_path = asset_path.with_file_name(format!(
        "{}.uuid",
        asset_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("asset")
    ));
    let _ = std::fs::remove_file(asset_path);
    let _ = std::fs::remove_file(sidecar_path);
}

fn inspect_image(path: &Path) -> CoreResult<InspectedImage> {
    let bytes = std::fs::read(path)?;
    if bytes.starts_with(b"<svg") || bytes.windows(4).any(|window| window == b"<svg") {
        return inspect_svg(&bytes);
    }

    let format = image::guess_format(&bytes)
        .map_err(|_| CoreError::parse("image", "unsupported or corrupt image data"))?;
    let (extension, media_type) = match format {
        image::ImageFormat::Png => ("png", "image/png"),
        image::ImageFormat::Jpeg => ("jpg", "image/jpeg"),
        image::ImageFormat::Gif => ("gif", "image/gif"),
        image::ImageFormat::WebP => ("webp", "image/webp"),
        _ => {
            return Err(CoreError::parse(
                "image",
                "only PNG, JPEG, GIF, WebP, and SVG are supported",
            ))
        }
    };
    debug_assert!(SUPPORTED_RASTER_EXTENSIONS.contains(&extension));
    let reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| CoreError::parse("image", "could not detect image format"))?;
    let dimensions = reader
        .into_dimensions()
        .map_err(|_| CoreError::parse("image", "could not read image dimensions"))?;
    validate_dimensions(dimensions)?;
    Ok(InspectedImage {
        object_type: ObjectType::Picture,
        extension,
        media_type,
        dimensions,
    })
}

fn inspect_svg(bytes: &[u8]) -> CoreResult<InspectedImage> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CoreError::parse("SVG", "SVG must be valid UTF-8"))?;
    let document = roxmltree::Document::parse(text)
        .map_err(|error| CoreError::parse("SVG", error.to_string()))?;
    let root = document.root_element();
    if root.tag_name().name() != "svg" {
        return Err(CoreError::parse("SVG", "document root must be <svg>"));
    }

    for node in document.descendants().filter(|node| node.is_element()) {
        let tag = node.tag_name().name().to_ascii_lowercase();
        if matches!(
            tag.as_str(),
            "script" | "foreignobject" | "iframe" | "object" | "embed"
        ) {
            return Err(CoreError::parse(
                "SVG",
                format!("active SVG element <{tag}> is not allowed"),
            ));
        }
        for attribute in node.attributes() {
            let name = attribute.name().to_ascii_lowercase();
            let value = attribute.value().trim().to_ascii_lowercase();
            if name.starts_with("on")
                || value.starts_with("javascript:")
                || ((name == "href" || name.ends_with(":href"))
                    && !value.is_empty()
                    && !value.starts_with('#')
                    && !value.starts_with("data:image/"))
                || value.contains("url(http:")
                || value.contains("url(https:")
            {
                return Err(CoreError::parse(
                    "SVG",
                    format!("external or active SVG attribute '{name}' is not allowed"),
                ));
            }
        }
    }

    let dimensions = svg_dimensions(&root)?;
    validate_dimensions(dimensions)?;
    Ok(InspectedImage {
        object_type: ObjectType::Drawing,
        extension: "svg",
        media_type: "image/svg+xml",
        dimensions,
    })
}

fn svg_dimensions(root: &roxmltree::Node<'_, '_>) -> CoreResult<(u32, u32)> {
    let width = root.attribute("width").and_then(parse_svg_length);
    let height = root.attribute("height").and_then(parse_svg_length);
    if let (Some(width), Some(height)) = (width, height) {
        return Ok((width, height));
    }
    let values = root
        .attribute("viewBox")
        .or_else(|| root.attribute("viewbox"))
        .map(|view_box| {
            view_box
                .split(|character: char| character.is_ascii_whitespace() || character == ',')
                .filter(|value| !value.is_empty())
                .filter_map(|value| value.parse::<f64>().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if values.len() == 4 && values[2] > 0.0 && values[3] > 0.0 {
        return Ok((values[2].round() as u32, values[3].round() as u32));
    }
    Err(CoreError::parse(
        "SVG",
        "SVG requires positive width/height or a valid viewBox",
    ))
}

fn parse_svg_length(value: &str) -> Option<u32> {
    let numeric = value
        .trim()
        .trim_end_matches(|character: char| character.is_ascii_alphabetic() || character == '%');
    let parsed = numeric.parse::<f64>().ok()?;
    (parsed.is_finite() && parsed > 0.0).then(|| parsed.round() as u32)
}

fn validate_dimensions((width, height): (u32, u32)) -> CoreResult<()> {
    if width == 0 || height == 0 {
        return Err(CoreError::parse(
            "image",
            "image dimensions must be positive",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> (tempfile::TempDir, Book) {
        let directory = tempfile::tempdir().unwrap();
        let book = Book::create(directory.path().join("book"), "Images").unwrap();
        (directory, book)
    }

    #[test]
    fn imports_safe_svg_as_drawing_without_modifying_source() {
        let (directory, book) = book();
        let source = directory.path().join("floor.anything");
        let original = br#"<svg viewBox="0 0 800 600"><g id="kitchen"><path d="M0 0"/></g></svg>"#;
        std::fs::write(&source, original).unwrap();

        let imported = import_image_object(&book, source.to_str().unwrap(), None).unwrap();
        assert_eq!(imported.object_type, ObjectType::Drawing);
        assert_eq!(imported.asset.unwrap().intrinsic_dimensions, (800, 600));
        assert_eq!(std::fs::read(source).unwrap(), original);
    }

    #[test]
    fn rejects_active_svg_and_leaves_no_assets() {
        let (directory, book) = book();
        let source = directory.path().join("unsafe.svg");
        std::fs::write(
            &source,
            r#"<svg viewBox="0 0 10 10"><script>alert(1)</script></svg>"#,
        )
        .unwrap();
        assert!(import_image_object(&book, source.to_str().unwrap(), None).is_err());
        assert!(!book.root.join(ASSETS_DIRECTORY).exists());
    }

    #[test]
    fn detects_png_content_despite_spoofed_extension() {
        let (directory, book) = book();
        let source = directory.path().join("photo.txt");
        image::DynamicImage::new_rgb8(3, 2)
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let imported = import_tracked_asset(&book, source.to_str().unwrap()).unwrap();
        assert_eq!(imported.media_type, "image/png");
        assert_eq!(imported.intrinsic_dimensions, (3, 2));
        assert!(imported.relative_path.ends_with(".png"));
    }

    #[test]
    fn corrupt_input_creates_nothing() {
        let (directory, book) = book();
        let source = directory.path().join("bad.png");
        std::fs::write(&source, b"not an image").unwrap();
        assert!(import_tracked_asset(&book, source.to_str().unwrap()).is_err());
        assert!(!book.root.join(ASSETS_DIRECTORY).exists());
    }
}
