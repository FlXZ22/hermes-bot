use crate::hermes::types::{HermesToolCall, ToolCallStatus};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Recorded language metadata for PTC syntax highlighting.
/// Strictly derived from recorded schema/tool metadata (never guessed from source code),
/// matching DeepSeek Harness `packages/client/ui-trajectory/src/client/code-program.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PtcLanguage {
    TypeScript,
    Python,
}

impl PtcLanguage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeScript => "typescript",
            Self::Python => "python",
        }
    }

    pub fn display_label(self) -> &'static str {
        match self {
            Self::TypeScript => "TypeScript",
            Self::Python => "Python",
        }
    }
}

/// Nested subcall executed inside a PTC program (`<parentCallId>:ptc:<n>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtcSubcall {
    pub sub_call_id: String,
    pub parent_call_id: String,
    pub tool_name: String,
    pub arguments_json: String,
    pub output_text: Option<String>,
    pub is_error: bool,
    pub duration_ms: Option<u64>,
}

/// Normalized Programmatic Tool Calling (`run_code`, `cua_run_code`, or Hermes `execute_code`)
/// record ready for DeepSeek-parity inspection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtcProgramInspection {
    pub tool_call_id: String,
    pub tool_name: String,
    pub summary: String,
    /// Exact verbatim source code (`arguments.code`), preserving trailing newlines & indentation.
    pub source: String,
    /// Exact verbatim recorded arguments JSON for the `{}` Original JSON toggle.
    pub raw_args_json: String,
    /// Highlighting language resolved strictly from recorded tool schema/metadata (`None` = plain text).
    pub language: Option<PtcLanguage>,
    /// Verbatim recorded output text; never split synthetically into stdout vs return value
    /// when the recording does not contain a split.
    pub output_text: Option<String>,
    pub output_is_json_container: bool,
    pub is_error: bool,
    pub is_running: bool,
    pub duration_ms: Option<u64>,
    pub subcalls: Vec<PtcSubcall>,
}

/// Match DeepSeek Harness `isProgramTool` plus Hermes's native `execute_code` PTC tool.
pub fn is_ptc_tool(tool_name: &str) -> bool {
    matches!(
        tool_name.trim(),
        "run_code" | "cua_run_code" | "execute_code"
    )
}

/// Resolve highlighting language strictly from recorded tool schema or explicit recorded metadata.
/// Mirrors `packages/client/ui-trajectory/src/client/code-program.ts:37-52`:
/// - Leading declaration: `Execute a program in \`<lang>\`` -> must be `typescript` or `python`;
///   any other explicit language returns `None` (never falls back to heuristics).
/// - Parameter description mention of `TypeScript` or `Python` (if both match, returns `None`).
/// - Hermes native `execute_code` is strictly Python 3.11 per `tools/code_execution_tool.py`.
pub fn resolve_recorded_language(
    tool_name: &str,
    schema_description: Option<&str>,
    code_param_description: Option<&str>,
    explicit_metadata_lang: Option<&str>,
) -> Option<PtcLanguage> {
    if let Some(meta) = explicit_metadata_lang.map(str::trim).filter(|s| !s.is_empty()) {
        return match meta.to_ascii_lowercase().as_str() {
            "typescript" | "ts" => Some(PtcLanguage::TypeScript),
            "python" | "py" => Some(PtcLanguage::Python),
            _ => None,
        };
    }

    if let Some(desc) = schema_description {
        let trimmed = desc.trim_start();
        if let Some(rest) = trimmed.strip_prefix("Execute a program in `") {
            if let Some(end) = rest.find('`') {
                let declared = rest[..end].trim().to_ascii_lowercase();
                return match declared.as_str() {
                    "typescript" => Some(PtcLanguage::TypeScript),
                    "python" => Some(PtcLanguage::Python),
                    _ => None,
                };
            }
        }
    }

    if let Some(param_desc) = code_param_description {
        let lower = param_desc.to_ascii_lowercase();
        let has_ts = contains_word(&lower, "typescript");
        let has_py = contains_word(&lower, "python");
        return match (has_ts, has_py) {
            (true, false) => Some(PtcLanguage::TypeScript),
            (false, true) => Some(PtcLanguage::Python),
            _ => None,
        };
    }

    // Hermes's built-in `execute_code` tool schema in v0.21.0 is exclusively Python.
    if tool_name.trim() == "execute_code" {
        return Some(PtcLanguage::Python);
    }

    None
}

fn contains_word(haystack_lower: &str, needle: &str) -> bool {
    for (idx, _) in haystack_lower.match_indices(needle) {
        let before_ok = idx == 0
            || !haystack_lower.as_bytes()[idx - 1].is_ascii_alphanumeric();
        let after = idx + needle.len();
        let after_ok = after == haystack_lower.len()
            || !haystack_lower.as_bytes()[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
    }
    false
}

/// Extract a DeepSeek-parity `PtcProgramInspection` from a recorded `HermesToolCall`.
pub fn inspect_ptc_tool_call(
    tool: &HermesToolCall,
    schema_description: Option<&str>,
    code_param_description: Option<&str>,
    subcalls: Vec<PtcSubcall>,
) -> Option<PtcProgramInspection> {
    if !is_ptc_tool(&tool.name) {
        return None;
    }

    let args = tool.arguments.as_ref()?;
    let obj = args.as_object()?;
    let source = obj.get("code")?.as_str()?.to_string();

    let raw_args_json = serde_json::to_string_pretty(args).unwrap_or_else(|_| args.to_string());
    let summary = obj
        .get("description")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .or_else(|| {
            source
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty())
                .map(|l| l.chars().take(90).collect())
        })
        .unwrap_or_else(|| tool.name.clone());

    let explicit_lang = obj.get("language").and_then(Value::as_str);
    let language = resolve_recorded_language(
        &tool.name,
        schema_description,
        code_param_description,
        explicit_lang,
    );

    let output_text = tool.output.clone();
    let is_error = tool.status == ToolCallStatus::Error;
    let output_is_json_container = !is_error
        && output_text
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s.trim()).ok())
            .is_some_and(|v| v.is_object() || v.is_array());

    Some(PtcProgramInspection {
        tool_call_id: tool.id.clone(),
        tool_name: tool.name.clone(),
        summary,
        source,
        raw_args_json,
        language,
        output_text,
        output_is_json_container,
        is_error,
        is_running: tool.status == ToolCallStatus::Running,
        duration_ms: tool.duration_ms,
        subcalls,
    })
}

/// Format nested PTC subcall ID following DeepSeek Harness `<parentCallId>:ptc:<n>` convention.
#[allow(dead_code)]
pub fn format_ptc_subcall_id(parent_call_id: &str, ordinal: usize) -> String {
    format!("{parent_call_id}:ptc:{ordinal}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn language_resolution_uses_only_recorded_schema_metadata_and_rejects_ambiguous_or_unknown() {
        assert_eq!(
            resolve_recorded_language(
                "run_code",
                Some("Execute a program in `typescript` to call tools."),
                None,
                None,
            ),
            Some(PtcLanguage::TypeScript)
        );
        assert_eq!(
            resolve_recorded_language(
                "run_code",
                Some("Execute a program in `ruby` to call tools."),
                Some("TypeScript code"),
                None,
            ),
            None
        );
        // Conflicting parameter mentions -> None (plain text)
        assert_eq!(
            resolve_recorded_language(
                "run_code",
                None,
                Some("Accepts TypeScript or Python"),
                None,
            ),
            None
        );
        // Hermes native execute_code is explicitly Python
        assert_eq!(
            resolve_recorded_language("execute_code", None, None, None),
            Some(PtcLanguage::Python)
        );
        // Unknown run_code without recorded schema -> None (never guesses from source)
        assert_eq!(
            resolve_recorded_language("run_code", None, None, None),
            None
        );
    }

    #[test]
    fn ptc_inspector_preserves_verbatim_source_original_json_and_unsplit_historical_output() {
        let code = "from hermes_tools import read_file\nprint(read_file('Cargo.toml'))\n";
        let tool = HermesToolCall {
            id: "tc-ptc-1".into(),
            name: "execute_code".into(),
            arguments: Some(json!({
                "code": code,
                "reset": false
            })),
            output: Some("{\"status\": \"error_text_preserved\"}".into()),
            status: ToolCallStatus::Error,
            duration_ms: Some(412),
            preview: None,
            inline_diff: None,
            waiting_approval: false,
        };
        let inspected = inspect_ptc_tool_call(&tool, None, None, vec![]).expect("inspect ptc");
        assert_eq!(inspected.source, code);
        assert_eq!(inspected.language, Some(PtcLanguage::Python));
        assert!(inspected.raw_args_json.contains("\"reset\": false"));
        // Error output must NOT be classified as JSON container — rendered verbatim as error pre
        assert!(inspected.is_error);
        assert!(!inspected.output_is_json_container);
        assert_eq!(
            inspected.output_text.as_deref(),
            Some("{\"status\": \"error_text_preserved\"}")
        );
        assert_eq!(format_ptc_subcall_id("tc-ptc-1", 1), "tc-ptc-1:ptc:1");
    }
}
