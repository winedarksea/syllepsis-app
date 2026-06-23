//! Building an ONNX Runtime [`Session`] for a model, on the execution provider the shared policy
//! chose (feature `onnx`).
//!
//! This binds the pure EP-selection of [`execution_provider`](super::execution_provider) to the
//! actual runtime: it asks `ort` which providers are *available* on this machine, runs the
//! policy to pick one (honoring the manifest's preference, CPU as the universal fallback), and
//! registers that provider plus CPU on the session — CPU last so a provider that fails to
//! initialize at load time still degrades to working CPU inference rather than erroring. The
//! resulting [`RuntimeDiagnostics`] travels with the session so the UI can report where inference
//! ran.

use std::path::Path;

// `OrtExecutionProvider` is the trait that provides `.is_available()` and `.build()` on the EP
// structs (`ep::CoreML`, `ep::CPU`, …); it must be in scope to call those methods.
use ort::ep::{self, coreml, ExecutionProvider as OrtExecutionProvider, ExecutionProviderDispatch};
use ort::session::{builder::GraphOptimizationLevel, Session};

use crate::error::{CoreError, CoreResult};
use crate::onnx::execution_provider::{
    select_execution_provider, ExecutionProvider, ExecutionProviderChoice, Platform,
};
use crate::onnx::manifest::ModelManifest;
use crate::onnx::RuntimeDiagnostics;

/// A loaded model session plus the diagnostics describing how it runs.
pub struct ModelSession {
    pub session: Session,
    pub diagnostics: RuntimeDiagnostics,
}

impl ModelSession {
    /// Load the model's weights file and place it on the chosen execution provider.
    pub fn load(weights_path: &Path, manifest: &ModelManifest) -> CoreResult<ModelSession> {
        let available = available_providers();
        let choice = select_execution_provider(
            &manifest.preferred_execution_providers,
            &available,
            Platform::host(),
        );
        log_execution_provider_choice(manifest, weights_path, &available, &choice);
        let mut builder = Session::builder()
            .map_err(map_ort_err)?
            .with_execution_providers(dispatches(&choice))
            .map_err(map_ort_err)?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(map_ort_err)?;
        let session = builder
            .commit_from_file(weights_path)
            .map_err(map_ort_err)?;
        Ok(ModelSession {
            session,
            diagnostics: RuntimeDiagnostics::new(manifest, &choice),
        })
    }
}

/// Ask the policy which EP to use, using the providers `ort` reports as available on this host.
pub fn resolve_execution_provider(manifest: &ModelManifest) -> ExecutionProviderChoice {
    select_execution_provider(
        &manifest.preferred_execution_providers,
        &available_providers(),
        Platform::host(),
    )
}

/// The accelerated providers actually available in this `ort` build/machine, plus CPU (always).
fn available_providers() -> Vec<ExecutionProvider> {
    let mut available = vec![ExecutionProvider::Cpu];
    if ep::CoreML::default().is_available().unwrap_or(false) {
        available.push(ExecutionProvider::CoreMl);
    }
    if ep::CUDA::default().is_available().unwrap_or(false) {
        available.push(ExecutionProvider::Cuda);
    }
    if ep::DirectML::default().is_available().unwrap_or(false) {
        available.push(ExecutionProvider::DirectMl);
    }
    available
}

/// The ort dispatch list for a choice: the accelerated provider first (if any), CPU always last.
fn dispatches(choice: &ExecutionProviderChoice) -> Vec<ExecutionProviderDispatch> {
    let mut list = Vec::new();
    match choice.provider {
        ExecutionProvider::CoreMl => list.push(coreml_dispatch()),
        ExecutionProvider::Cuda => list.push(ep::CUDA::default().build()),
        ExecutionProvider::DirectMl => list.push(ep::DirectML::default().build()),
        ExecutionProvider::Cpu => {}
    }
    list.push(ep::CPU::default().build());
    list
}

fn coreml_dispatch() -> ExecutionProviderDispatch {
    let provider = ep::CoreML::default().with_compute_units(coreml::ComputeUnits::All);
    if coreml_profile_enabled() {
        provider.with_profile_compute_plan(true).build()
    } else {
        provider.build()
    }
}

fn coreml_profile_enabled() -> bool {
    std::env::var("SYLLEPSIS_ONNX_COREML_PROFILE")
        .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn log_execution_provider_choice(
    manifest: &ModelManifest,
    weights_path: &Path,
    available: &[ExecutionProvider],
    choice: &ExecutionProviderChoice,
) {
    let available = provider_list(available);
    let preferred = provider_list(&manifest.preferred_execution_providers);
    let coreml_profile = coreml_profile_enabled();
    tracing::info!(
        model = %manifest.id,
        graph = %weights_path.display(),
        selected_execution_provider = choice.provider.as_str(),
        used_cpu_fallback = choice.used_cpu_fallback,
        available_execution_providers = %available,
        preferred_execution_providers = %preferred,
        coreml_compute_units = if choice.provider == ExecutionProvider::CoreMl { "all" } else { "n/a" },
        coreml_profile_compute_plan = coreml_profile,
        "onnx: loading model session"
    );
}

fn provider_list(providers: &[ExecutionProvider]) -> String {
    providers
        .iter()
        .map(|provider| provider.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

/// Map an `ort` error (generic over its recovery type `R`) onto the crate error type.
pub(crate) fn map_ort_err<R>(e: ort::Error<R>) -> CoreError {
    CoreError::Model(format!("onnx runtime: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_onnx_build_exposes_coreml_provider() {
        assert!(
            available_providers().contains(&ExecutionProvider::CoreMl),
            "macOS ONNX builds must enable ORT's coreml feature or local models silently fall back to CPU"
        );
    }

    #[test]
    fn provider_list_is_log_friendly() {
        assert_eq!(
            provider_list(&[ExecutionProvider::Cpu, ExecutionProvider::CoreMl]),
            "cpu,coreml"
        );
    }
}
