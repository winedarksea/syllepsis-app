//! Generate the trusted Equal Earth SVG layers bundled by the frontend.
//!
//! Usage:
//! cargo run -p syllepsis-core --example generate_earth_basemap -- \
//!   input.geojson output.svg detail-label

use std::fmt::Write as _;
use std::path::Path;

use serde_json::Value;
use syllepsis_core::spatial::equal_earth_normalized;

const WIDTH: f64 = 1_200.0;
const HEIGHT: f64 = 620.0;

fn main() {
    let arguments = std::env::args().collect::<Vec<_>>();
    if arguments.len() != 4 {
        eprintln!("expected: input.geojson output.svg detail-label");
        std::process::exit(2);
    }
    let input = std::fs::read_to_string(&arguments[1]).expect("read GeoJSON");
    let geojson: Value = serde_json::from_str(&input).expect("parse GeoJSON");
    let svg = generate_svg(&geojson, &arguments[3]);
    if let Some(parent) = Path::new(&arguments[2]).parent() {
        std::fs::create_dir_all(parent).expect("create output directory");
    }
    std::fs::write(&arguments[2], &svg).expect("write SVG");
    eprintln!(
        "wrote {} bytes to {}",
        svg.len(),
        Path::new(&arguments[2]).display()
    );
}

fn generate_svg(geojson: &Value, detail_label: &str) -> String {
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {WIDTH} {HEIGHT}" data-source="Natural Earth" data-detail="{detail_label}"><g fill="#819683" fill-rule="evenodd" stroke="#f0eee7" stroke-width=".42" stroke-linejoin="round">"##
    );
    for feature in geojson["features"].as_array().into_iter().flatten() {
        let name = feature["properties"]["ADMIN"]
            .as_str()
            .or_else(|| feature["properties"]["NAME"].as_str())
            .unwrap_or("country");
        let id = sanitize_id(name);
        let geometry = &feature["geometry"];
        let mut path = String::new();
        append_geometry_path(geometry, &mut path);
        if !path.is_empty() {
            let _ = write!(svg, r#"<path id="{id}" d="{path}"/>"#);
        }
    }
    svg.push_str("</g></svg>");
    svg
}

fn append_geometry_path(geometry: &Value, output: &mut String) {
    let coordinates = &geometry["coordinates"];
    match geometry["type"].as_str() {
        Some("Polygon") => append_polygon(coordinates, output),
        Some("MultiPolygon") => {
            for polygon in coordinates.as_array().into_iter().flatten() {
                append_polygon(polygon, output);
            }
        }
        _ => {}
    }
}

fn append_polygon(polygon: &Value, output: &mut String) {
    for ring in polygon.as_array().into_iter().flatten() {
        let Some(points) = ring.as_array() else {
            continue;
        };
        let mut first = true;
        let mut previous_x = 0.0;
        for coordinate in points {
            let Some(pair) = coordinate.as_array() else {
                continue;
            };
            let (Some(longitude), Some(latitude)) = (
                pair.first().and_then(Value::as_f64),
                pair.get(1).and_then(Value::as_f64),
            ) else {
                continue;
            };
            let (normalized_x, normalized_y) = equal_earth_normalized(longitude, latitude);
            let x = normalized_x * WIDTH;
            let y = normalized_y * HEIGHT;
            if first || (x - previous_x).abs() > WIDTH * 0.55 {
                let _ = write!(output, "M{:.2},{:.2}", x, y);
                first = false;
            } else {
                let _ = write!(output, "L{:.2},{:.2}", x, y);
            }
            previous_x = x;
        }
        if !first {
            output.push('Z');
        }
    }
}

fn sanitize_id(name: &str) -> String {
    let mut id = String::from("country-");
    let mut separator = false;
    for character in name.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            id.push(character);
            separator = false;
        } else if !separator {
            id.push('-');
            separator = true;
        }
    }
    while id.ends_with('-') {
        id.pop();
    }
    id
}
