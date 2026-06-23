use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Bake the build date into the binary for the Settings → About panel. Honor
    // SOURCE_DATE_EPOCH for reproducible builds, otherwise use the current time.
    let epoch = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|delta| delta.as_secs() as i64)
                .unwrap_or(0)
        });
    println!(
        "cargo:rustc-env=SYLLEPSIS_BUILD_DATE={}",
        format_iso_date(epoch)
    );
    // Without these, the date freezes at the first build; "last full rebuild" is acceptable.
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");

    tauri_build::build()
}

/// Convert a Unix timestamp (seconds) to a UTC `YYYY-MM-DD` string using Howard Hinnant's
/// civil-from-days algorithm, so build.rs stays free of a date-crate dependency.
fn format_iso_date(epoch_secs: i64) -> String {
    let days = epoch_secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}
