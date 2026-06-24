//! Picking which ONNX Runtime execution provider (EP) to run a model on, and recording the
//! choice for Diagnostics.
//!
//! ONNX Runtime can dispatch a graph to several backends — Apple CoreML, Windows DirectML,
//! NVIDIA CUDA, or the always-present CPU kernels. Which are *available* depends on the build
//! and machine; which are *preferred* depends on the platform and the model (a manifest lists
//! the EPs it is known to run well on, best first). This module is the pure policy that
//! intersects the two and always lands on a concrete choice, because the CPU provider is a
//! universal fallback. Keeping it free of `ort` means the decision is unit-testable without a
//! runtime present — the feature-gated [`session`](super::session) layer just applies the result.

use serde::{Deserialize, Serialize};

/// An ONNX Runtime execution provider (hardware backend).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionProvider {
    /// Apple Neural Engine / GPU via CoreML (macOS, iOS).
    CoreMl,
    /// Windows GPU via DirectML.
    DirectMl,
    /// NVIDIA GPU via CUDA.
    Cuda,
    /// Portable CPU kernels. Always available; the universal fallback.
    Cpu,
}

impl ExecutionProvider {
    /// Stable identifier used in diagnostics and config.
    pub fn as_str(self) -> &'static str {
        match self {
            ExecutionProvider::CoreMl => "coreml",
            ExecutionProvider::DirectMl => "directml",
            ExecutionProvider::Cuda => "cuda",
            ExecutionProvider::Cpu => "cpu",
        }
    }

    /// Whether this provider runs on a hardware accelerator (vs. plain CPU). Used by the UI to
    /// warn that local inference will be slow when only the CPU fallback is available.
    pub fn is_accelerated(self) -> bool {
        !matches!(self, ExecutionProvider::Cpu)
    }
}

/// The host platform, which bounds the set of EPs that could ever be available.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
    Android,
    /// The PWA build, where inference goes through `onnxruntime-web` (WebGPU/WASM) instead of
    /// these native EPs; modeled so callers can branch without a separate code path.
    Web,
}

impl Platform {
    /// The platform this build is compiled for. Web is never inferred here — it is a build
    /// target the native crate is not compiled into — so only native platforms are returned.
    pub fn host() -> Platform {
        #[cfg(target_os = "macos")]
        {
            Platform::MacOs
        }
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }
        #[cfg(target_os = "android")]
        {
            Platform::Android
        }
        #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
        {
            Platform::Linux
        }
    }

    /// EPs that could plausibly exist on this platform, best first, CPU always last. This is the
    /// *candidate* set; whether an accelerated EP is actually present is reported separately by
    /// the runtime and intersected in [`select_execution_provider`].
    pub fn candidate_providers(self) -> Vec<ExecutionProvider> {
        use ExecutionProvider::*;
        match self {
            Platform::MacOs => vec![CoreMl, Cpu],
            Platform::Windows => vec![DirectMl, Cuda, Cpu],
            Platform::Linux => vec![Cuda, Cpu],
            // Android native inference falls back to CPU in this POC (NNAPI is a later add).
            Platform::Android => vec![Cpu],
            // The PWA does not use these native EPs at all.
            Platform::Web => vec![Cpu],
        }
    }
}

/// The outcome of EP selection, kept for Diagnostics so the user can see exactly where local
/// inference ran and whether it fell back to CPU.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionProviderChoice {
    /// The provider that will run the model.
    pub provider: ExecutionProvider,
    /// True when no accelerated provider the model prefers was available, so CPU was chosen as
    /// the fallback. The UI surfaces this to set performance expectations.
    pub used_cpu_fallback: bool,
}

/// Choose the execution provider for a model.
///
/// The result is the first provider that is in **all three** of: the model's preference order
/// (`model_preference`, best first), the platform's candidate set, and the set actually
/// `available` at runtime. A non-empty model preference is also its accelerated-provider
/// compatibility allow-list: an unlisted platform accelerator is not tried. An empty preference
/// delegates to the platform's normal ordering. If no accelerated provider survives, CPU is used.
pub fn select_execution_provider(
    model_preference: &[ExecutionProvider],
    available: &[ExecutionProvider],
    platform: Platform,
) -> ExecutionProviderChoice {
    let candidates = platform.candidate_providers();
    let is_usable = |ep: ExecutionProvider| {
        ep != ExecutionProvider::Cpu && candidates.contains(&ep) && available.contains(&ep)
    };

    let chosen_accelerated = if model_preference.is_empty() {
        candidates.iter().copied().find(|&ep| is_usable(ep))
    } else {
        model_preference.iter().copied().find(|&ep| is_usable(ep))
    };

    match chosen_accelerated {
        Some(provider) => ExecutionProviderChoice {
            provider,
            used_cpu_fallback: false,
        },
        None => ExecutionProviderChoice {
            provider: ExecutionProvider::Cpu,
            used_cpu_fallback: true,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::ExecutionProvider::*;
    use super::*;

    #[test]
    fn prefers_model_ranked_provider_when_available() {
        // Model prefers CUDA; on Windows with both DirectML and CUDA present, CUDA wins because
        // the model's preference order is honored ahead of the platform default (DirectML).
        let choice =
            select_execution_provider(&[Cuda, DirectMl], &[DirectMl, Cuda], Platform::Windows);
        assert_eq!(choice.provider, Cuda);
        assert!(!choice.used_cpu_fallback);
    }

    #[test]
    fn falls_back_to_cpu_when_no_accelerator_available() {
        // CoreML is preferred and a macOS candidate, but the runtime reports it unavailable.
        let choice = select_execution_provider(&[CoreMl], &[], Platform::MacOs);
        assert_eq!(choice.provider, Cpu);
        assert!(choice.used_cpu_fallback);
    }

    #[test]
    fn ignores_providers_not_valid_for_the_platform() {
        // CUDA is "available" but macOS has no CUDA candidate, so CoreML is used instead.
        let choice = select_execution_provider(&[Cuda, CoreMl], &[Cuda, CoreMl], Platform::MacOs);
        assert_eq!(choice.provider, CoreMl);
        assert!(!choice.used_cpu_fallback);
    }

    #[test]
    fn does_not_use_an_accelerator_omitted_from_model_compatibility() {
        let choice = select_execution_provider(&[Cuda, DirectMl], &[CoreMl, Cpu], Platform::MacOs);
        assert_eq!(choice.provider, Cpu);
        assert!(choice.used_cpu_fallback);
    }

    #[test]
    fn uses_platform_order_when_model_has_no_preference() {
        let choice = select_execution_provider(&[], &[DirectMl, Cuda], Platform::Windows);
        assert_eq!(
            choice.provider, DirectMl,
            "platform ranks DirectML before CUDA"
        );
    }

    #[test]
    fn android_always_uses_cpu_fallback() {
        let choice = select_execution_provider(&[Cuda], &[Cuda], Platform::Android);
        assert_eq!(choice.provider, Cpu);
        assert!(choice.used_cpu_fallback);
    }

    #[test]
    fn accelerated_flag_distinguishes_cpu() {
        assert!(CoreMl.is_accelerated());
        assert!(!Cpu.is_accelerated());
    }
}
