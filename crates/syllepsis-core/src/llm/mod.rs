//! Optional LLM acceleration, routed per task and gated behind the [`LlmProvider`] seam.
//!
//! The pipeline is: pick the task ([`task::LlmTask`]) → route it to a model
//! ([`crate::config::LlmRouting`]) → build the prompt ([`prompts`]) → call the provider
//! ([`provider::LlmProvider`]) → wrap the reply as a [`proposal::Proposal`] the user accepts or
//! rejects. The built-in [`offline::OfflineLlmProvider`] makes the whole flow work and be
//! tested with no network; a real Claude/local provider is added as another `impl LlmProvider`
//! without changing anything above it.

pub mod chat;
pub mod offline;
pub mod prompts;
pub mod proposal;
pub mod provider;
pub mod selection;
pub mod service;
pub mod task;

#[cfg(feature = "onnx")]
pub mod onnx;

pub use offline::OfflineLlmProvider;
#[cfg(feature = "onnx")]
pub use onnx::OnnxLlmProvider;
pub use proposal::{Proposal, ProposalStatus};
pub use provider::{LlmProvider, LlmRequest, LlmResponse};
pub use selection::{select_llm_provider, LOCAL_PROVIDER};
pub use service::{parse_category_list, LlmService};
pub use task::LlmTask;
