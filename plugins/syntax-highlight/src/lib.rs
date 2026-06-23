//! A small, dependency-free syntax highlighter, compiled to WASM and run by the Syllepsis plugin
//! host as a `code_block_renderer`. It demonstrates the render hook: given a fenced block's
//! language tag and source, it returns HTML that the app sanitizes and shows in place of the raw
//! block.
//!
//! The highlighting is intentionally simple (keywords, strings, line comments, numbers) — enough to
//! be useful and to prove the contract without pulling a heavy grammar engine into the WASM module.

use extism_pdk::*;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct RenderInput {
    language: String,
    code: String,
}

#[derive(Serialize)]
struct RenderOutput {
    html: String,
}

#[plugin_fn]
pub fn render(input: Json<RenderInput>) -> FnResult<Json<RenderOutput>> {
    let Json(input) = input;
    let inner = highlight(&input.language, &input.code);
    let html = format!(
        "<pre class=\"plugin-codeblock\" data-language=\"{}\"><code>{}</code></pre>",
        escape_html(&input.language),
        inner
    );
    Ok(Json(RenderOutput { html }))
}

/// Highlight `code` for `language`, returning HTML for the inside of a `<code>` element. Every
/// character is HTML-escaped; recognized tokens are wrapped in `<span class="tok-…">`.
fn highlight(language: &str, code: &str) -> String {
    let keywords = keywords_for(language);
    let mut out = String::with_capacity(code.len() * 2);
    let chars: Vec<char> = code.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // Line comments: // … or # …
        if (c == '/' && chars.get(i + 1) == Some(&'/')) || c == '#' {
            let start = i;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            let text: String = chars[start..i].iter().collect();
            out.push_str(&span("comment", &text));
            continue;
        }
        // Strings: "…" or '…'
        if c == '"' || c == '\'' {
            let quote = c;
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                i += 1;
            }
            let text: String = chars[start..i.min(chars.len())].iter().collect();
            out.push_str(&span("string", &text));
            continue;
        }
        // Identifiers / keywords
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if keywords.contains(&word.as_str()) {
                out.push_str(&span("keyword", &word));
            } else {
                out.push_str(&escape_html(&word));
            }
            continue;
        }
        // Numbers
        if c.is_ascii_digit() {
            let start = i;
            while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_')
            {
                i += 1;
            }
            let num: String = chars[start..i].iter().collect();
            out.push_str(&span("number", &num));
            continue;
        }
        // Anything else: escape a single character.
        out.push_str(&escape_html(&c.to_string()));
        i += 1;
    }
    out
}

fn span(class: &str, text: &str) -> String {
    format!("<span class=\"tok-{}\">{}</span>", class, escape_html(text))
}

fn keywords_for(language: &str) -> &'static [&'static str] {
    match language.to_lowercase().as_str() {
        "rust" | "rs" => &[
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "trait", "use", "mod", "match",
            "if", "else", "for", "while", "loop", "return", "self", "Self", "crate", "super",
            "const", "static", "async", "await", "move", "ref", "where", "dyn", "as", "in",
            "break", "continue", "type", "unsafe",
        ],
        "python" | "py" => &[
            "def", "class", "return", "if", "elif", "else", "for", "while", "import", "from", "as",
            "with", "try", "except", "finally", "raise", "lambda", "yield", "pass", "break",
            "continue", "and", "or", "not", "in", "is", "None", "True", "False", "global",
        ],
        "javascript" | "js" | "typescript" | "ts" => &[
            "const", "let", "var", "function", "return", "if", "else", "for", "while", "switch",
            "case", "break", "continue", "class", "extends", "new", "this", "import", "export",
            "from", "default", "async", "await", "try", "catch", "finally", "throw", "typeof",
            "instanceof", "void", "null", "undefined", "true", "false",
        ],
        _ => &[],
    }
}

fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}
