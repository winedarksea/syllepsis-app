# Android build setup

Android is its own phase: the CI workflow (`.github/workflows/android.yml`) and the config
overlay (`crates/syllepsis-tauri/tauri.android.conf.json`) are in place, but two things must be
done **locally** (they need the Android NDK and iterative compilation) before the workflow can
build green.

## What's already wired

- **`tauri.android.conf.json`** — overrides `beforeBuildCommand` to skip model prep and ensure
  an empty `bundled-models` dir exists, so the APK stays small. The runtime HuggingFace
  download fallback (`model_bootstrap.rs`) provides the model on first use.
- **`Cargo.toml` target split** — on `target_os = "android"`, `syllepsis-core` is pulled with
  only the `loro` feature (no `extism`/wasmtime, no `onnx`/ort), and `keyring` (no Android
  backend) is desktop-only.
- **`android.yml`** — JDK 17 + Android SDK + NDK 27, builds `--apk --aab`, signs from a
  gitignored `keystore.properties`, uploads to the tag's draft release. iOS stub commented out.

## Remaining local work

### 1. cfg-gate the Android-excluded features in source

Because Android drops `extism`, `onnx`, and `keyring`, every `syllepsis-tauri` code path that
uses them must be gated for `target_os = "android"` (or moved behind the desktop feature set).
Expect to touch:

- **`src/secrets.rs`** — keyring-backed credential storage; provide an Android stub or gate it.
- Semantic-search / embeddings command paths that call into the `onnx` feature.
- WASM plugin runtime paths that call into the `extism` feature.

Iterate with the Android target until it compiles:

```sh
rustup target add aarch64-linux-android wasm32-wasip1
cd crates/syllepsis-tauri
cargo check --target aarch64-linux-android --config tauri.android.conf.json
```

> The **open risk** is `ort`/onnxruntime linking on Android. If it can't be made to link, the
> fallback is desktop-only semantic search on Android initially — which the feature split above
> already assumes (no `onnx` on Android).

### 2. Initialize and commit the Android project

```sh
export ANDROID_HOME="$HOME/Library/Android/sdk"
export NDK_HOME="$ANDROID_HOME/ndk/27.0.12077973"
cd crates/syllepsis-tauri
cargo tauri android init
```

Then edit `gen/android/app/build.gradle.kts` to read signing config from a gitignored
`keystore.properties` (the CI workflow writes this file from secrets):

```kotlin
import java.util.Properties
import java.io.FileInputStream

val keystorePropertiesFile = rootProject.file("keystore.properties")
val keystoreProperties = Properties()
if (keystorePropertiesFile.exists()) {
    keystoreProperties.load(FileInputStream(keystorePropertiesFile))
}

android {
    signingConfigs {
        create("release") {
            if (keystorePropertiesFile.exists()) {
                storeFile = file(keystoreProperties["storeFile"] as String)
                storePassword = keystoreProperties["storePassword"] as String
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["keyPassword"] as String
            }
        }
    }
    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.getByName("release")
        }
    }
}
```

Commit `gen/android` (minus `keystore.properties`, which is gitignored).

### 3. Generate the upload keystore

```sh
keytool -genkey -v -keystore ~/syllepsis-upload.jks \
  -keyalg RSA -keysize 2048 -validity 10000 -alias upload
```

Keep a safe local copy. base64-encode it into the `ANDROID_KEYSTORE_BASE64` secret and set
`ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`. In Play Console,
create the app, upload the first AAB manually, and enroll in Play App Signing.

## Verify

```sh
cd crates/syllepsis-tauri
cargo tauri android build --apk --aab --target aarch64 --config tauri.android.conf.json
```

Install the APK on a device and confirm the app launches and (with a connected account) syncs.
