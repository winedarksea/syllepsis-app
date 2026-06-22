//! The pure text shaping around the bundled local LLM: building the Gemma prompt and cleaning
//! the model's raw output.
//!
//! Kept free of the runtime ([`onnx`](super::onnx)) so it is unit-testable without a model. A
//! chat model does not see plain text — it sees a sequence of role-delimited turns. Gemma uses
//! `<start_of_turn>{role}\n…<end_of_turn>`, ending with an open model turn the model completes.
//! If a system prompt is provided it is folded into the user turn (the recommended practice for
//! Gemma models without a dedicated system turn). Centralizing both prompt building and output
//! cleaning here means the decode loop only has to run tokens in and tokens out.

/// Gemma turn delimiters.
pub const TURN_START: &str = "<start_of_turn>";
pub const TURN_END: &str = "<end_of_turn>";
/// The token the model emits to end its turn; decoding stops here.
pub const STOP_TOKEN: &str = TURN_END;

/// Build a Gemma prompt ending in an open model turn for the model to complete. The system
/// prompt, when non-blank, is prepended to the user message in the user turn (the recommended
/// Gemma pattern for single-turn tasks). A blank `system` is omitted.
pub fn build_prompt(system: &str, user: &str) -> String {
    let user_content = if system.trim().is_empty() {
        user.trim().to_string()
    } else {
        format!("{}\n\n{}", system.trim(), user.trim())
    };
    format!("{TURN_START}user\n{user_content}{TURN_END}\n{TURN_START}model\n")
}

/// Remove a `<think>…</think>` reasoning block from generated text and trim. Handles three cases:
/// a well-formed block (drop it, keep what follows), a block left open by truncation (drop
/// everything from `<think>` on — there is no real answer yet), and no block (return trimmed).
/// This is a no-op for Gemma 4 E2B (which does not emit thinking blocks) but is kept so the
/// pipeline stays correct if the manifest is swapped to a reasoning model.
pub fn strip_thinking(output: &str) -> String {
    const OPEN: &str = "<think>";
    const CLOSE: &str = "</think>";
    match output.find(OPEN) {
        None => output.trim().to_string(),
        Some(start) => match output[start..].find(CLOSE) {
            Some(rel_end) => {
                let after = &output[start + rel_end + CLOSE.len()..];
                let before = &output[..start];
                format!("{before}{after}").trim().to_string()
            }
            None => output[..start].trim().to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chatml_includes_system_then_user_then_open_assistant() {
        let p = build_prompt("Be terse.", "Summarize this note.");
        assert_eq!(
            p,
            "<start_of_turn>user\nBe terse.\n\nSummarize this note.<end_of_turn>\n\
             <start_of_turn>model\n"
        );
    }

    #[test]
    fn blank_system_is_omitted() {
        let p = build_prompt("   ", "hello");
        assert!(!p.contains("system"));
        assert!(p.starts_with("<start_of_turn>user\nhello"));
        assert!(p.ends_with("<start_of_turn>model\n"));
    }

    #[test]
    fn strips_well_formed_think_block() {
        let out = "<think>let me reason about this</think>The answer is 42.";
        assert_eq!(strip_thinking(out), "The answer is 42.");
    }

    #[test]
    fn drops_truncated_open_think_block() {
        let out = "<think>still reasoning and ran out of tokens";
        assert_eq!(strip_thinking(out), "");
    }

    #[test]
    fn passes_through_plain_output() {
        assert_eq!(strip_thinking("  just an answer  "), "just an answer");
    }
}
