//! Validated image ingestion shared by inline Markdown images, Picture/Drawing objects, and worlds.

use std::path::{Path, PathBuf};

use image::ImageReader;
use serde::{Deserialize, Serialize};

use crate::app::NoteDto;
use crate::error::{CoreError, CoreResult};
use crate::model::{AssetMetadata, Note, ObjectType};
use crate::storage::{Book, NoteStore};
use crate::sync::{assign_asset_uuid, AssetRegistry};

const ASSETS_DIRECTORY: &str = "assets";
const SUPPORTED_RASTER_EXTENSIONS: &[&str] = &["png", "jpg", "gif", "webp"];

#[derive(Debug, Clone, PartialEq, Eq)]
struct InspectedImage {
    object_type: ObjectType,
    extension: &'static str,
    media_type: &'static str,
    dimensions: (u32, u32),
    normalized_bytes: Option<Vec<u8>>,
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

    if let Some(bytes) = &inspected.normalized_bytes {
        std::fs::write(&temporary_path, bytes)?;
    } else {
        std::fs::copy(source, &temporary_path)?;
    }
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

/// Create a new blank Drawing note with a minimal starter SVG containing an empty embedded scene.
/// The note's body is empty; callers may populate it later (e.g., with linked-note references).
pub fn create_drawing_object(book: &Book, title: &str) -> CoreResult<NoteDto> {
    let blank_svg = blank_drawing_svg();
    let imported = write_asset_bytes(book, blank_svg.as_bytes(), "svg", "image/svg+xml", (800, 600), "drawing.svg")?;
    let note_title = {
        let t = title.trim();
        if t.is_empty() { "Drawing".to_string() } else { t.to_string() }
    };
    let mut note = Note::new(
        ObjectType::Drawing,
        note_title,
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

/// Overwrite the SVG asset for an existing Drawing note. The SVG is validated by `inspect_svg`
/// before writing. The asset UUID/filename are preserved; only the file contents change.
/// Returns the updated note DTO (dimensions may change if the new SVG differs).
pub fn save_drawing_svg(book: &Book, note_id: &str, svg: &str) -> CoreResult<NoteDto> {
    let id = crate::id::NoteId::parse(note_id)?;
    let mut note = book.store.read_note(&id)?;
    if note.object_type != ObjectType::Drawing {
        return Err(CoreError::InvalidBook(format!(
            "note '{note_id}' is not a Drawing"
        )));
    }
    let Some(asset) = &note.asset else {
        return Err(CoreError::InvalidBook(format!(
            "Drawing '{note_id}' has no asset"
        )));
    };
    let registry = AssetRegistry::scan(&book.root)?;
    let relative_path = registry.resolve(&asset.uuid).ok_or_else(|| {
        CoreError::InvalidBook(format!("asset UUID '{}' not found in registry", asset.uuid))
    })?;
    let asset_path = book.root.join(relative_path);

    // Validate the SVG (also normalises the prolog). inspect_svg returns InspectedImage.
    let inspected = inspect_svg(svg.as_bytes())?;
    let final_bytes = inspected
        .normalized_bytes
        .unwrap_or_else(|| svg.as_bytes().to_vec());

    // Atomic write: temp → rename so a crash never leaves a half-written file.
    let temp_path = asset_path.with_extension("svg.tmp");
    std::fs::write(&temp_path, &final_bytes).map_err(|e| {
        let _ = std::fs::remove_file(&temp_path);
        CoreError::Io(e)
    })?;
    if let Err(e) = std::fs::rename(&temp_path, &asset_path) {
        let _ = std::fs::remove_file(&temp_path);
        return Err(CoreError::Io(e));
    }

    // Refresh dimensions in case the canvas was resized.
    let current_asset = note.asset.as_mut().unwrap();
    current_asset.intrinsic_dimensions = inspected.dimensions;
    note.metadata.dates.updated = chrono::Utc::now();
    book.save_note(&note)?;
    Ok(NoteDto::from_note(&note))
}

/// Return the raw SVG text for the Drawing note's asset file. Used by the frontend to seed the
/// Excalidraw editor. Returns an error if the note is not a Drawing or has no asset.
pub fn read_drawing_svg(book: &Book, note_id: &str) -> CoreResult<String> {
    let id = crate::id::NoteId::parse(note_id)?;
    let note = book.store.read_note(&id)?;
    if note.object_type != ObjectType::Drawing {
        return Err(CoreError::InvalidBook(format!(
            "note '{note_id}' is not a Drawing"
        )));
    }
    let Some(asset) = &note.asset else {
        return Err(CoreError::InvalidBook(format!(
            "Drawing '{note_id}' has no asset"
        )));
    };
    let registry = AssetRegistry::scan(&book.root)?;
    let relative_path = registry.resolve(&asset.uuid).ok_or_else(|| {
        CoreError::InvalidBook(format!("asset UUID '{}' not found in registry", asset.uuid))
    })?;
    let text = std::fs::read_to_string(book.root.join(relative_path))?;
    Ok(text)
}

/// Write raw bytes as a new tracked asset (temp → uuid → rename). Returns ImportedAsset on
/// success. On failure the temp file (if written) is cleaned up before returning the error.
fn write_asset_bytes(
    book: &Book,
    bytes: &[u8],
    extension: &str,
    media_type: &str,
    dimensions: (u32, u32),
    original_filename: &str,
) -> CoreResult<ImportedAsset> {
    let assets_directory = book.root.join(ASSETS_DIRECTORY);
    std::fs::create_dir_all(&assets_directory)?;
    let file_stem = ulid::Ulid::new().to_string().to_lowercase();
    let file_name = format!("{file_stem}.{extension}");
    let relative_path = format!("{ASSETS_DIRECTORY}/{file_name}");
    let final_path = assets_directory.join(&file_name);
    let temporary_path = assets_directory.join(format!(".{file_name}.importing"));

    std::fs::write(&temporary_path, bytes)?;
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
        media_type: media_type.to_string(),
        intrinsic_dimensions: dimensions,
        original_filename: original_filename.to_string(),
        object_type: ObjectType::Drawing,
    })
}

/// A minimal valid SVG with an empty Excalidraw scene embedded in `<metadata>`.
/// The scene JSON is embedded as plain text, matching exactly what the frontend save
/// path writes (`metaEl.textContent = serializeAsJSON(...)`), so a freshly created
/// drawing reads back identically to a re-saved one.
fn blank_drawing_svg() -> String {
    let scene = r#"{"type":"excalidraw","version":2,"source":"syllepsis","elements":[],"appState":{"gridSize":null,"viewBackgroundColor":"transparent"},"files":{}}"#;
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 800 600" width="800" height="600"><metadata>{scene}</metadata></svg>"#
    )
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

/// Delete any `.{name}.importing` temp files left behind by a crashed import.
pub fn cleanup_stale_imports(book: &Book) -> CoreResult<()> {
    let assets_dir = book.root.join(ASSETS_DIRECTORY);
    if !assets_dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&assets_dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with('.') && name.ends_with(".importing") {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    Ok(())
}

/// Delete the asset file and `.uuid` sidecar for every inline image referenced in `note_body`.
/// Best-effort: missing files are silently skipped.
pub fn delete_inline_assets(book_root: &Path, note_body: &str) {
    for path in extract_inline_asset_paths(note_body) {
        let _ = std::fs::remove_file(book_root.join(&path));
        let _ = std::fs::remove_file(book_root.join(format!("{path}.uuid")));
    }
}

/// Delete tracked assets that are not referenced by any note in the book.
/// Assets modified within the last 5 minutes are left alone (sync grace window).
pub fn delete_orphaned_assets(book: &Book) -> CoreResult<()> {
    delete_orphaned_assets_as_of(book, std::time::SystemTime::now())
}

fn delete_orphaned_assets_as_of(book: &Book, now: std::time::SystemTime) -> CoreResult<()> {
    let registry = AssetRegistry::scan(&book.root)?;
    if registry.is_empty() {
        return Ok(());
    }

    let mut referenced = std::collections::HashSet::new();
    let all_notes = book
        .store
        .read_all_notes()?
        .into_iter()
        .chain(book.read_all_commentary_notes()?);
    for note in all_notes {
        if let Some(asset) = &note.asset {
            referenced.insert(asset.uuid.clone());
        }
        for path in extract_inline_asset_paths(&note.body) {
            let sidecar = book.root.join(format!("{path}.uuid"));
            if let Ok(uuid) = std::fs::read_to_string(&sidecar) {
                referenced.insert(uuid.trim().to_string());
            }
        }
    }

    let grace = std::time::Duration::from_secs(5 * 60);
    for (uuid, relative_path) in registry.entries() {
        if referenced.contains(uuid) {
            continue;
        }
        let asset_path = book.root.join(relative_path);
        if let Ok(metadata) = std::fs::metadata(&asset_path) {
            if let Ok(modified) = metadata.modified() {
                if now.duration_since(modified).unwrap_or_default() < grace {
                    continue;
                }
            }
        }
        let _ = std::fs::remove_file(&asset_path);
        let _ = std::fs::remove_file(book.root.join(format!("{relative_path}.uuid")));
    }
    Ok(())
}

fn extract_inline_asset_paths(body: &str) -> Vec<String> {
    let mut paths = Vec::new();
    const PREFIX: &str = "](assets/";
    let mut remaining = body;
    while let Some(pos) = remaining.find(PREFIX) {
        remaining = &remaining[pos + 2..]; // skip past "](" → "assets/..."
        if let Some(end) = remaining.find(')') {
            paths.push(remaining[..end].to_string());
            remaining = &remaining[end + 1..];
        } else {
            break;
        }
    }
    paths
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
        normalized_bytes: None,
    })
}

fn inspect_svg(bytes: &[u8]) -> CoreResult<InspectedImage> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| CoreError::parse("SVG", "SVG must be valid UTF-8"))?;
    let normalized = normalize_svg_text(text)?;
    let document = roxmltree::Document::parse(normalized)
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
                    && !value.starts_with("data:image/")
                    && !is_allowed_syllepsis_href(attribute.value().trim()))
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
        normalized_bytes: (normalized.as_ptr() != text.as_ptr())
            .then(|| normalized.as_bytes().to_vec()),
    })
}

/// Returns true for `syllepsis://note/<ulid-or-id>` hrefs produced by the in-app drawing editor.
/// The id must be non-empty and contain only URL-safe identifier characters so no path-traversal
/// or injection is possible through this allowance.
fn is_allowed_syllepsis_href(href: &str) -> bool {
    let Some(id) = href.strip_prefix("syllepsis://note/") else {
        return false;
    };
    !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn normalize_svg_text(text: &str) -> CoreResult<&str> {
    let svg_start = text
        .find("<svg")
        .ok_or_else(|| CoreError::parse("SVG", "document root must be <svg>"))?;
    Ok(&text[svg_start..])
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
    if value.trim().ends_with('%') {
        return None;
    }
    let numeric = value
        .trim()
        .trim_end_matches(|character: char| character.is_ascii_alphabetic());
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
    fn imports_svg_with_xml_and_dtd_prolog_as_normalized_drawing() {
        let (directory, book) = book();
        let source = directory.path().join("map.svg");
        let original = r#"<?xml version="1.0" encoding="UTF-8" standalone="no"?>
<!DOCTYPE svg PUBLIC "-//W3C//DTD SVG 1.1//EN" "http://www.w3.org/Graphics/SVG/1.1/DTD/svg11.dtd">
<svg width="100%" height="100%" viewBox="0 0 13334 10000" xmlns="http://www.w3.org/2000/svg">
  <g id="coastline"><path d="M0 0L10 10"/></g>
</svg>"#;
        std::fs::write(&source, original).unwrap();

        let imported = import_image_object(&book, source.to_str().unwrap(), None).unwrap();
        let asset = imported.asset.unwrap();
        assert_eq!(imported.object_type, ObjectType::Drawing);
        assert_eq!(asset.intrinsic_dimensions, (13334, 10000));
        assert_eq!(std::fs::read_to_string(&source).unwrap(), original);

        let registry = crate::sync::AssetRegistry::scan(&book.root).unwrap();
        let relative_path = registry.resolve(&asset.uuid).unwrap();
        let stored = std::fs::read_to_string(book.root.join(relative_path)).unwrap();
        assert!(stored.starts_with("<svg"));
        assert!(!stored.contains("<!DOCTYPE"));
    }

    #[test]
    fn rejects_svg_that_depends_on_dtd_entities() {
        let (directory, book) = book();
        let source = directory.path().join("entity.svg");
        std::fs::write(
            &source,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE svg [
  <!ENTITY label "Kitchen">
]>
<svg viewBox="0 0 10 10"><text>&label;</text></svg>"#,
        )
        .unwrap();

        assert!(import_image_object(&book, source.to_str().unwrap(), None).is_err());
        assert!(!book.root.join(ASSETS_DIRECTORY).exists());
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

    #[test]
    fn stale_import_temp_deleted_on_cleanup() {
        let (_dir, book) = book();
        let assets_dir = book.root.join(ASSETS_DIRECTORY);
        std::fs::create_dir_all(&assets_dir).unwrap();
        let temp_file = assets_dir.join(".foo.importing");
        std::fs::write(&temp_file, b"partial").unwrap();

        cleanup_stale_imports(&book).unwrap();
        assert!(!temp_file.exists());
    }

    #[test]
    fn real_asset_not_touched_by_import_cleanup() {
        let (directory, book) = book();
        let source = directory.path().join("photo.png");
        image::DynamicImage::new_rgb8(3, 2)
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let imported = import_tracked_asset(&book, source.to_str().unwrap()).unwrap();
        let asset_path = book.root.join(&imported.relative_path);
        assert!(asset_path.exists());

        cleanup_stale_imports(&book).unwrap();
        assert!(asset_path.exists());
    }

    #[test]
    fn delete_inline_assets_removes_file_and_sidecar() {
        let (directory, book) = book();
        let source = directory.path().join("photo.png");
        image::DynamicImage::new_rgb8(3, 2)
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let a = import_tracked_asset(&book, source.to_str().unwrap()).unwrap();
        let b = import_tracked_asset(&book, source.to_str().unwrap()).unwrap();

        let body = format!("![image]({})", a.relative_path);
        delete_inline_assets(&book.root, &body);

        assert!(
            !book.root.join(&a.relative_path).exists(),
            "referenced asset deleted"
        );
        assert!(
            !book.root.join(format!("{}.uuid", a.relative_path)).exists(),
            "referenced sidecar deleted"
        );
        assert!(
            book.root.join(&b.relative_path).exists(),
            "unreferenced asset intact"
        );
        assert!(
            book.root.join(format!("{}.uuid", b.relative_path)).exists(),
            "unreferenced sidecar intact"
        );
    }

    #[test]
    fn orphan_scan_removes_unreferenced_assets() {
        let (directory, book) = book();
        let source = directory.path().join("photo.png");
        image::DynamicImage::new_rgb8(3, 2)
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let imported = import_tracked_asset(&book, source.to_str().unwrap()).unwrap();
        let asset_path = book.root.join(&imported.relative_path);
        let sidecar_path = book.root.join(format!("{}.uuid", imported.relative_path));
        assert!(asset_path.exists());
        assert!(sidecar_path.exists());

        // Use a far-future "now" so the 5-minute grace window has long elapsed.
        let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        delete_orphaned_assets_as_of(&book, far_future).unwrap();

        assert!(!asset_path.exists(), "orphaned asset removed");
        assert!(!sidecar_path.exists(), "orphaned sidecar removed");
    }

    #[test]
    fn orphan_scan_keeps_referenced_assets() {
        let (directory, book) = book();
        let source = directory.path().join("photo.png");
        image::DynamicImage::new_rgb8(3, 2)
            .save_with_format(&source, image::ImageFormat::Png)
            .unwrap();
        let imported = import_image_object(&book, source.to_str().unwrap(), None).unwrap();
        let asset = imported.asset.as_ref().unwrap();
        let registry = crate::sync::AssetRegistry::scan(&book.root).unwrap();
        let relative_path = registry.resolve(&asset.uuid).unwrap().to_string();
        let asset_path = book.root.join(&relative_path);
        let sidecar_path = book.root.join(format!("{relative_path}.uuid"));
        assert!(asset_path.exists());
        assert!(sidecar_path.exists());

        let far_future = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
        delete_orphaned_assets_as_of(&book, far_future).unwrap();

        assert!(asset_path.exists(), "referenced asset kept");
        assert!(sidecar_path.exists(), "referenced sidecar kept");
    }

    // ── Drawing-specific tests ────────────────────────────────────────────────

    #[test]
    fn create_drawing_object_produces_drawing_note_with_svg_asset() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "My Canvas").unwrap();
        assert_eq!(dto.object_type, ObjectType::Drawing);
        assert_eq!(dto.title, "My Canvas");
        let asset = dto.asset.unwrap();
        assert_eq!(asset.media_type, "image/svg+xml");
        assert_eq!(asset.intrinsic_dimensions, (800, 600));
        let registry = crate::sync::AssetRegistry::scan(&book.root).unwrap();
        let rel = registry.resolve(&asset.uuid).unwrap();
        let stored_svg = std::fs::read_to_string(book.root.join(rel)).unwrap();
        assert!(stored_svg.contains("<svg"), "stored file is an SVG");
        assert!(stored_svg.contains("excalidraw"), "SVG contains embedded scene");
    }

    #[test]
    fn create_drawing_object_blank_title_defaults_to_drawing() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "  ").unwrap();
        assert_eq!(dto.title, "Drawing");
    }

    #[test]
    fn save_drawing_svg_overwrites_asset_and_preserves_uuid() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "canvas").unwrap();
        let original_uuid = dto.asset.as_ref().unwrap().uuid.clone();

        let new_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300" width="400" height="300"><rect width="10" height="10"/></svg>"#;
        let updated = save_drawing_svg(&book, &dto.id, new_svg).unwrap();

        assert_eq!(updated.asset.as_ref().unwrap().uuid, original_uuid, "uuid unchanged");
        assert_eq!(updated.asset.as_ref().unwrap().intrinsic_dimensions, (400, 300));
    }

    #[test]
    fn save_drawing_svg_accepts_syllepsis_note_href() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "links").unwrap();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 100" width="100" height="100"><a xlink:href="syllepsis://note/01ABCDEFGHJKLMNPQRST"><rect width="10" height="10"/></a></svg>"#;
        save_drawing_svg(&book, &dto.id, svg).unwrap();
    }

    #[test]
    fn save_drawing_svg_rejects_script_tags_and_leaves_prior_file_intact() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "secure").unwrap();
        let asset_uuid = dto.asset.as_ref().unwrap().uuid.clone();
        let registry = crate::sync::AssetRegistry::scan(&book.root).unwrap();
        let rel = registry.resolve(&asset_uuid).unwrap().to_string();
        let original_content = std::fs::read_to_string(book.root.join(&rel)).unwrap();

        let malicious = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"><script>alert(1)</script></svg>"#;
        assert!(save_drawing_svg(&book, &dto.id, malicious).is_err());

        let after = std::fs::read_to_string(book.root.join(rel)).unwrap();
        assert_eq!(after, original_content, "prior file intact after rejection");
    }

    #[test]
    fn save_drawing_svg_rejects_external_href() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "external").unwrap();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 100 100" width="100" height="100"><a xlink:href="https://evil.example/x"><rect width="10" height="10"/></a></svg>"#;
        assert!(save_drawing_svg(&book, &dto.id, svg).is_err());
    }

    #[test]
    fn save_drawing_svg_rejects_non_drawing_note() {
        let (_dir, book) = book();
        let text_note = crate::app::commands::create_note(&book, ObjectType::Note, "text note", None).unwrap();
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100" width="100" height="100"></svg>"#;
        assert!(save_drawing_svg(&book, &text_note.id, svg).is_err());
    }

    #[test]
    fn read_drawing_svg_returns_raw_svg_text() {
        let (_dir, book) = book();
        let dto = create_drawing_object(&book, "read test").unwrap();
        let new_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 150" width="200" height="150"><circle cx="50" cy="50" r="10"/></svg>"#;
        save_drawing_svg(&book, &dto.id, new_svg).unwrap();
        let read_back = read_drawing_svg(&book, &dto.id).unwrap();
        assert!(read_back.contains("<svg"), "returns SVG text");
        assert!(read_back.contains("<circle"), "contains saved element");
    }

    #[test]
    fn syllepsis_href_rule_allows_valid_ids() {
        assert!(is_allowed_syllepsis_href("syllepsis://note/01ABCDEFGHJKLMNPQRST"));
        assert!(is_allowed_syllepsis_href("syllepsis://note/abc-def_123"));
    }

    #[test]
    fn syllepsis_href_rule_rejects_malformed() {
        assert!(!is_allowed_syllepsis_href("syllepsis://note/"));
        assert!(!is_allowed_syllepsis_href("syllepsis://note/../etc"));
        assert!(!is_allowed_syllepsis_href("https://example.com"));
        assert!(!is_allowed_syllepsis_href("javascript:alert(1)"));
    }
}
