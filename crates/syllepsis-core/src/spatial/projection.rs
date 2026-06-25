//! Equal Earth projection used by the offline Earth world and map-data generator.

use std::f64::consts::{FRAC_PI_2, PI};

const A1: f64 = 1.340_264;
const A2: f64 = -0.081_106;
const A3: f64 = 0.000_893;
const A4: f64 = 0.003_796;
const M: f64 = 0.866_025_403_784_438_6;
const NEWTON_TOLERANCE: f64 = 1.0e-12;
const NEWTON_ITERATIONS: usize = 12;

fn projected_extents() -> (f64, f64) {
    let maximum_x = PI / (M * A1);
    let theta = (M * FRAC_PI_2.sin()).asin();
    let theta_squared = theta * theta;
    let theta_sixth = theta_squared * theta_squared * theta_squared;
    let maximum_y = theta * (A1 + A2 * theta_squared + theta_sixth * (A3 + A4 * theta_squared));
    (maximum_x, maximum_y)
}

/// Project longitude/latitude degrees into Equal Earth planar coordinates.
pub fn equal_earth_forward(longitude_degrees: f64, latitude_degrees: f64) -> (f64, f64) {
    let longitude = longitude_degrees.clamp(-180.0, 180.0).to_radians();
    let latitude = latitude_degrees.clamp(-90.0, 90.0).to_radians();
    let theta = (M * latitude.sin()).asin();
    let theta_squared = theta * theta;
    let theta_sixth = theta_squared * theta_squared * theta_squared;
    let denominator =
        M * (A1 + 3.0 * A2 * theta_squared + theta_sixth * (7.0 * A3 + 9.0 * A4 * theta_squared));
    let x = longitude * theta.cos() / denominator;
    let y = theta * (A1 + A2 * theta_squared + theta_sixth * (A3 + A4 * theta_squared));
    (x, y)
}

/// Invert Equal Earth planar coordinates into `(longitude, latitude)` degrees.
pub fn equal_earth_inverse(x: f64, y: f64) -> Option<(f64, f64)> {
    let (maximum_x, maximum_y) = projected_extents();
    if !x.is_finite() || !y.is_finite() || x.abs() > maximum_x * 1.01 || y.abs() > maximum_y * 1.01
    {
        return None;
    }

    let mut theta = y / A1;
    for _ in 0..NEWTON_ITERATIONS {
        let theta_squared = theta * theta;
        let theta_sixth = theta_squared * theta_squared * theta_squared;
        let function =
            theta * (A1 + A2 * theta_squared + theta_sixth * (A3 + A4 * theta_squared)) - y;
        let derivative =
            A1 + 3.0 * A2 * theta_squared + theta_sixth * (7.0 * A3 + 9.0 * A4 * theta_squared);
        let adjustment = function / derivative;
        theta -= adjustment;
        if adjustment.abs() < NEWTON_TOLERANCE {
            break;
        }
    }
    let cosine = theta.cos();
    if cosine.abs() < NEWTON_TOLERANCE {
        return None;
    }
    let theta_squared = theta * theta;
    let theta_sixth = theta_squared * theta_squared * theta_squared;
    let longitude = x
        * M
        * (A1 + 3.0 * A2 * theta_squared + theta_sixth * (7.0 * A3 + 9.0 * A4 * theta_squared))
        / cosine;
    let latitude = (theta.sin() / M).clamp(-1.0, 1.0).asin();
    if longitude.abs() > PI * 1.01 || latitude.abs() > FRAC_PI_2 * 1.01 {
        return None;
    }
    Some((longitude.to_degrees(), latitude.to_degrees()))
}

/// Project longitude/latitude to normalized stage coordinates with north at the top.
pub fn equal_earth_normalized(longitude_degrees: f64, latitude_degrees: f64) -> (f64, f64) {
    let (x, y) = equal_earth_forward(longitude_degrees, latitude_degrees);
    let (maximum_x, maximum_y) = projected_extents();
    (((x / maximum_x) + 1.0) * 0.5, (1.0 - (y / maximum_y)) * 0.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_projects_to_center() {
        assert_eq!(equal_earth_forward(0.0, 0.0), (0.0, 0.0));
        assert_eq!(equal_earth_normalized(0.0, 0.0), (0.5, 0.5));
    }

    #[test]
    fn forward_inverse_round_trip_reference_points() {
        for (longitude, latitude) in [
            (0.0, 0.0),
            (-122.3321, 47.6062),
            (2.3522, 48.8566),
            (179.5, -45.0),
            (-180.0, 0.0),
            (0.0, 89.999),
        ] {
            let (x, y) = equal_earth_forward(longitude, latitude);
            let (actual_longitude, actual_latitude) = equal_earth_inverse(x, y).unwrap();
            assert!((longitude - actual_longitude).abs() < 1.0e-8);
            assert!((latitude - actual_latitude).abs() < 1.0e-8);
            let (normalized_x, normalized_y) = equal_earth_normalized(longitude, latitude);
            assert!((0.0..=1.0).contains(&normalized_x));
            assert!((0.0..=1.0).contains(&normalized_y));
        }
    }

    #[test]
    fn rejects_coordinates_outside_projection() {
        assert!(equal_earth_inverse(100.0, 100.0).is_none());
    }
}
