buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:8.11.0")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:1.9.25")
    }
}

fun findRustlsPlatformVerifierMavenRepository(): File {
    val cargoMetadata = providers.exec {
        workingDir = file("../..")
        commandLine(
            "cargo", "metadata", "--format-version", "1",
            "--filter-platform", "aarch64-linux-android",
            "--manifest-path", "Cargo.toml",
        )
    }.standardOutput.asText.get()
    val manifestPath = Regex("\\\"manifest_path\\\":\\\"([^\\\"]*rustls-platform-verifier-android[^\\\"]*)\\\"")
        .find(cargoMetadata)
        ?.groupValues
        ?.get(1)
        ?: error("Cargo did not resolve rustls-platform-verifier-android")
    return File(File(manifestPath).parentFile, "maven")
}

allprojects {
    repositories {
        google()
        mavenCentral()
        maven(url = findRustlsPlatformVerifierMavenRepository()) {
            metadataSources { artifact() }
        }
    }
}

tasks.register("clean").configure {
    delete("build")
}
