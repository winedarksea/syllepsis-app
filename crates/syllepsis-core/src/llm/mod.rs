//! Optional LLM acceleration, routed per task and gated behind the [`LlmProvider`] seam.
//!
//! The pipeline is: pick the task ([`task::LlmTask`]) → route it to a model
//! ([`crate::config::LlmRouting`]) → build the prompt ([`prompts`]) → call the provider
//! ([`provider::LlmProvider`]) → wrap the reply as a [`proposal::Proposal`] the user accepts or
//! rejects. Provider implementations must be model-backed: the bundled local ONNX model runs
//! in-process, while cloud/server providers execute in the desktop shell and re-enter as
//! proposals.

pub mod chat;
pub mod prompts;
pub mod proposal;
pub mod provider;
pub mod selection;
pub mod service;
pub mod task;

#[cfg(feature = "onnx")]
pub mod onnx;

#[cfg(feature = "onnx")]
pub use onnx::OnnxLlmProvider;
pub use proposal::{Proposal, ProposalStatus};
pub use provider::{LlmProvider, LlmRequest, LlmResponse};
pub use selection::{select_llm_provider, LOCAL_PROVIDER};
pub use service::{parse_category_list, LlmService};
pub use task::LlmTask;
