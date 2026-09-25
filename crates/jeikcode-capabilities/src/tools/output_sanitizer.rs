//! General-purpose build/command output sanitizer and semantic diff folder.
//!
//! Multi-language coverage:
//! - Android / Gradle / Kotlin / Java
//! - Python (pytest, unittest, mypy, Traceback)
//! - TypeScript / JavaScript (Node, Bun, Vite, Jest, Vitest, tsc)
//! - Rust (cargo, rustc)
//! - Go (go build, go test, testify)
//! - C / C++ (GCC, Clang, CMake, Ninja, Make, GoogleTest)
//! - C# / .NET / MSBuild
//! - Shell & CLI scripts (Bash, PowerShell)
//!
//! Core Guarantees:
//! 1. 100% preserve all errors, panics, tracebacks, line numbers, and failure diagnostics.
//! 2. Fold giant test assertion reflection dumps (left vs right) into concise unified diffs.
//! 3. Suppress redundant scrolling compilation logs and static warnings for clean success/failure.
//! 4. Seamlessly integrate with [`super::output_artifact::ArtifactStore`] for full raw recall.

use similar::{ChangeTag, TextDiff};

/// Maximum preview lines of identical unchanged context around a diff hunk.
const DIFF_CONTEXT_LINES: usize = 3;
/// Fold identical block if it has more than this number of unchanged lines.
const FOLD_IDENTICAL_THRESHOLD: usize = 5;

/// Known error markers across all supported language runtimes and build tools.
/// When any of these are present, the failure diagnostic zone is 100% PRESERVED.
const ERROR_MARKERS: &[&str] = &[
    // Rust
    "error[",
    "error:",
    "panicked at",
    "failures:",
    // Python
    "Traceback (most recent call last):",
    "AssertionError:",
    "SyntaxError:",
    "NameError:",
    "TypeError:",
    "ValueError:",
    "IndentationError:",
    // Android / Gradle / Java
    "BUILD FAILED",
    "FAILURE: Build failed",
    "[ERROR]",
    "Compilation failure",
    "Exception in thread",
    "Caused by:",
    "AndroidLint: Error",
    "AAPT: error:",
    // TypeScript / JavaScript / Node
    "FAIL ",
    "Error: ",
    "error TS",
    // Go
    "--- FAIL: ",
    "panic: ",
    "FAIL\t",
    // C / C++ (GCC / Clang / MSVC / Make / Ninja)
    "fatal error:",
    ": error C",
    "make: ***",
    "ninja: build stopped",
    "undefined reference to",
    // C# (.NET / MSBuild)
    "Build FAILED",
    ": error CS",
    ": error MSB",
    // Shell / Script / Generic
    "command not found",
    "No such file or directory",
    "Permission denied",
];

/// Checks if a line represents compilation progress across major build systems.
fn is_compilation_progress(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Rust / Cargo
    trimmed.starts_with("Compiling ")
        || trimmed.starts_with("Checking ")
        || trimmed.starts_with("Downloaded ")
        || trimmed.starts_with("Building ")
    // Android / Gradle
        || trimmed.starts_with("> Task :")
        || (trimmed.starts_with("[task ") && trimmed.contains(":]"))
        || trimmed.starts_with("Download https://")
        || trimmed.starts_with("Downloading https://")
    // Java / Maven
        || trimmed.starts_with("[INFO] Compiling ")
        || trimmed.starts_with("[INFO] Building ")
        || trimmed.starts_with("[INFO] Scanning for projects")
    // C / C++ (Make / CMake / Ninja)
        || (trimmed.starts_with("[") && trimmed.contains("%] Building "))
        || (trimmed.starts_with("[") && trimmed.contains("%] Linking "))
        || trimmed.starts_with("Scanning dependencies of target")
    // Go
        || trimmed.starts_with("go: downloading ")
    // C# / .NET / MSBuild
        || trimmed.starts_with("Determining projects to restore...")
        || trimmed.starts_with("Restored ")
    // Node / Web (npm / pnpm / yarn / bun / vite)
        || trimmed.starts_with("[1/4] Resolving packages")
        || trimmed.starts_with("[2/4] Fetching packages")
        || trimmed.starts_with("[3/4] Linking dependencies")
        || trimmed.starts_with("[4/4] Building fresh packages")
        || trimmed.starts_with("transforming...")
        || trimmed.starts_with("chunking...")
}

/// Checks if a line starts a compiler/linter warning block across supported languages.
fn is_compiler_warning_start(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Rust / GCC / Clang
    trimmed.starts_with("warning:")
        || trimmed.starts_with("warning[")
        || trimmed.starts_with("Warning:")
    // Java / Maven
        || trimmed.starts_with("[WARNING]")
    // Android / Kotlin / Gradle
        || trimmed.starts_with("w: ")
        || trimmed.starts_with("w: [")
        || trimmed.starts_with("w: /")
    // C# / .NET / MSBuild
        || trimmed.contains(": warning CS")
        || trimmed.contains(": warning MSB")
    // TypeScript / ESLint / Python
        || trimmed.starts_with("warning  ")
        || trimmed.contains("UserWarning:")
        || trimmed.contains("DeprecationWarning:")
        || trimmed.contains("FutureWarning:")
}

/// Checks if a line is transient connection/terminal wrapper noise (SSH, PTY, Docker).
fn is_transient_connection_noise(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("Warning: Permanently added '")
        || trimmed.starts_with("Pseudo-terminal will not be allocated")
        || (trimmed.starts_with("Connection to ") && trimmed.ends_with("closed."))
        || trimmed.starts_with("mesg: ttyname failed")
        || trimmed.starts_with("bash: cannot set terminal process group")
}

/// Checks if a line contains a generic source code location error pattern (e.g. `path/file.ext:123:45:`).
/// Language agnostic: covers GCC, Clang, Rust, Go, Python, TS, C#, Java, Swift, etc.
fn is_source_location_error(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with("-->")
        || trimmed.contains("warning")
        || trimmed.contains("Warning")
        || trimmed.contains("[WARNING]")
        || trimmed.starts_with("w:")
    {
        return false;
    }
    if trimmed.contains(": error")
        || trimmed.contains(": fatal error")
        || trimmed.contains("error:")
        || trimmed.starts_with("File \"")
    {
        return true;
    }
    if let Some(first_colon) = trimmed.find(':') {
        if first_colon > 1 {
            let after = &trimmed[first_colon + 1..];
            if let Some(second_colon) = after.find(':') {
                let line_str = &after[..second_colon];
                if let Ok(num) = line_str.parse::<u32>() {
                    if num > 0 {
                        let remainder = &after[second_colon + 1..];
                        if !remainder.contains("warning") && !remainder.contains("note") {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

fn normalize_dump_lines(s: &str) -> String {
    if s.contains("\\n") && !s.contains('\n') {
        s.replace("\\n", "\n")
    } else if s.len() > 500 && s.lines().count() <= 3 && s.contains(", ") {
        s.replace(", ", ",\n")
    } else {
        s.to_string()
    }
}
fn render_unified_diff(
    header: &str,
    left_raw: &str,
    right_raw: &str,
    prefix: &str,
    suffix: &str,
    id_hint: &str,
) -> String {
    let left_norm = normalize_dump_lines(left_raw);
    let right_norm = normalize_dump_lines(right_raw);
    let diff = TextDiff::from_lines(&left_norm, &right_norm);
    let mut diff_output = String::with_capacity(4096);

    diff_output.push_str(&format!(
        "\n{header} (semantic diff: actual vs expected):\n--- left (actual)\n+++ right (expected)\n"
    ));

    let mut identical_run: Vec<&str> = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                identical_run.push(change.value());
            }
            ChangeTag::Delete => {
                flush_identical(&mut diff_output, &mut identical_run, id_hint);
                diff_output.push_str("- ");
                diff_output.push_str(change.value());
                if !change.value().ends_with('\n') {
                    diff_output.push('\n');
                }
            }
            ChangeTag::Insert => {
                flush_identical(&mut diff_output, &mut identical_run, id_hint);
                diff_output.push_str("+ ");
                diff_output.push_str(change.value());
                if !change.value().ends_with('\n') {
                    diff_output.push('\n');
                }
            }
        }
    }
    flush_identical(&mut diff_output, &mut identical_run, id_hint);

    format!("{prefix}{diff_output}{suffix}")
}

fn flush_identical(out: &mut String, run: &mut Vec<&str>, id_hint: &str) {
    if run.is_empty() {
        return;
    }
    if run.len() <= FOLD_IDENTICAL_THRESHOLD {
        for line in run.drain(..) {
            out.push(' ');
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push('\n');
            }
        }
    } else {
        let head_slice = &run[..DIFF_CONTEXT_LINES.min(run.len())];
        for line in head_slice {
            out.push(' ');
            out.push_str(line);
            if !line.ends_with('\n') {
                out.push('\n');
            }
        }
        let elided = run.len() - (head_slice.len() * 2).min(run.len());
        if elided > 0 {
            out.push_str(&format!(
                " [... {elided} lines of identical fields folded.{id_hint}]\n"
            ));
        }
        let tail_start = run.len().saturating_sub(DIFF_CONTEXT_LINES);
        if tail_start > head_slice.len() {
            for line in &run[tail_start..] {
                out.push(' ');
                out.push_str(line);
                if !line.ends_with('\n') {
                    out.push('\n');
                }
            }
        }
        run.clear();
    }
}

/// Fold giant test assertion reflection dumps across Rust, Go, Java, Python, C#, etc.
pub fn fold_assertion_diff(content: &str, artifact_id: Option<&str>) -> Option<String> {
    let id_hint = artifact_id
        .map(|id| format!(" Full raw dump in artifact {id}."))
        .unwrap_or_default();

    // 1. Rust assertion pattern: left vs right
    let rust_headers = [
        "assertion `left == right` failed",
        "assertion failed: `(left == right)`",
        "assertion failed: left == right",
    ];
    for pat in &rust_headers {
        if let Some(pos) = content.find(pat) {
            let remainder = &content[pos..];
            let left_patterns = ["\n  left: ", "\n left: ", "\nleft: "];
            let right_patterns = ["\n right: ", "\nright: ", "\n  right: "];

            let mut left_info = None;
            for lp in &left_patterns {
                if let Some(p) = remainder.find(lp) {
                    left_info = Some((p, lp.len()));
                    break;
                }
            }
            if let Some((left_pos, left_len)) = left_info {
                let after_left = &remainder[left_pos + left_len..];
                let mut right_info = None;
                for rp in &right_patterns {
                    if let Some(p) = after_left.find(rp) {
                        right_info = Some((p, rp.len()));
                        break;
                    }
                }
                if let Some((right_rel_pos, right_len)) = right_info {
                    let right_pos = left_pos + left_len + right_rel_pos;
                    let left_raw = &remainder[left_pos + left_len..right_pos];
                    let after_right = &remainder[right_pos + right_len..];
                    let right_end = if let Some(p) = after_right.find("\nnote:") {
                        p
                    } else if let Some(p) = after_right.find("\nfailures:") {
                        p
                    } else {
                        after_right.len()
                    };
                    let right_raw = &after_right[..right_end];
                    let suffix = &after_right[right_end..];
                    if left_raw.len() + right_raw.len() >= 600 {
                        return Some(render_unified_diff(
                            pat,
                            left_raw,
                            right_raw,
                            &content[..pos],
                            suffix,
                            &id_hint,
                        ));
                    }
                }
            }
        }
    }

    // 2. Go (testify / assert) & C# (NUnit) & Java (JUnit / AssertJ): expected vs actual
    let generic_headers = [
        "Not equal:",
        "AssertionError: expected:",
        "expected: <",
        "Expected: <",
    ];
    for gh in &generic_headers {
        if let Some(pos) = content.find(gh) {
            let remainder = &content[pos..];
            let exp_markers = [
                "\n  expected: ",
                "\n expected: ",
                "\nexpected: ",
                "\n  Expected: ",
            ];
            let act_markers = [
                "\n  actual  : ",
                "\n actual: ",
                "\nactual: ",
                "\n  But was:  ",
                "\n  but was: ",
            ];

            let mut exp_info = None;
            for ep in &exp_markers {
                if let Some(p) = remainder.find(ep) {
                    exp_info = Some((p, ep.len()));
                    break;
                }
            }
            if let Some((exp_pos, exp_len)) = exp_info {
                let after_exp = &remainder[exp_pos + exp_len..];
                let mut act_info = None;
                for ap in &act_markers {
                    if let Some(p) = after_exp.find(ap) {
                        act_info = Some((p, ap.len()));
                        break;
                    }
                }
                if let Some((act_rel_pos, act_len)) = act_info {
                    let act_pos = exp_pos + exp_len + act_rel_pos;
                    let exp_raw = &remainder[exp_pos + exp_len..act_pos];
                    let after_act = &remainder[act_pos + act_len..];
                    let act_end = after_act.find("\n\n").unwrap_or(after_act.len());
                    let act_raw = &after_act[..act_end];
                    let suffix = &after_act[act_end..];
                    if exp_raw.len() + act_raw.len() >= 600 {
                        return Some(render_unified_diff(
                            gh,
                            exp_raw,
                            act_raw,
                            &content[..pos],
                            suffix,
                            &id_hint,
                        ));
                    }
                }
            }
        }
    }

    None
}

/// Generic build and test log sanitizer across all programming ecosystems.
pub fn sanitize_build_output(
    content: &str,
    is_failure: bool,
    artifact_id: Option<&str>,
) -> Option<String> {
    if content.len() < 512 {
        return None;
    }

    let id_hint = artifact_id
        .map(|id| format!(" Full log in artifact {id}."))
        .unwrap_or_default();

    if !is_failure {
        // --- Success Case: condense repetitive compiling roll and noisy warnings ---
        let mut clean_lines: Vec<&str> = Vec::new();
        let mut compiling_count = 0;
        let mut warning_count = 0;
        let mut in_warning_block = false;
        let mut warning_samples = Vec::new();

        for line in content.lines() {
            if is_transient_connection_noise(line) {
                continue;
            }
            if is_compilation_progress(line) {
                compiling_count += 1;
                in_warning_block = false;
                continue;
            }

            if is_compiler_warning_start(line) {
                warning_count += 1;
                in_warning_block = true;
                if warning_samples.len() < 2 {
                    warning_samples.push(line.trim());
                }
                continue;
            }

            if in_warning_block {
                let trimmed = line.trim_start();
                if trimmed.starts_with("-->")
                    || trimmed.starts_with("=")
                    || trimmed.starts_with("|")
                    || trimmed.starts_with("at ")
                    || trimmed.chars().next().is_some_and(|c| c.is_ascii_digit())
                    || trimmed.is_empty()
                {
                    continue;
                } else {
                    in_warning_block = false;
                }
            }

            clean_lines.push(line);
        }

        if compiling_count > 3 || warning_count > 2 {
            let mut result = String::with_capacity(content.len() / 2);
            if compiling_count > 0 {
                result.push_str(&format!(
                    "✓ [Compiled/Built {compiling_count} steps successfully]{id_hint}\n"
                ));
            }
            if warning_count > 0 {
                result.push_str(&format!(
                    "⚠ [{warning_count} compiler warnings suppressed (e.g. '{}')]{id_hint}\n",
                    warning_samples
                        .first()
                        .copied()
                        .unwrap_or("unused variable/import")
                ));
            }
            for l in clean_lines {
                result.push_str(l);
                result.push('\n');
            }
            return Some(result.trim_end().to_string());
        }
    } else {
        // --- Failure Case: 100% preserve errors and stacktraces, condense pre-error warnings ---
        let mut first_error_pos = None;
        for marker in ERROR_MARKERS {
            if let Some(pos) = content.find(marker) {
                first_error_pos = Some(first_error_pos.map_or(pos, |p: usize| p.min(pos)));
            }
        }

        // Also detect generic source location error line if earlier (e.g. `file.ext:123:45:`)
        let mut char_offset = 0;
        for line in content.lines() {
            if is_source_location_error(line) {
                first_error_pos =
                    Some(first_error_pos.map_or(char_offset, |p: usize| p.min(char_offset)));
                break;
            }
            char_offset += line.len() + 1;
        }

        if let Some(err_pos) = first_error_pos {
            let pre_error = &content[..err_pos];
            let error_and_tail = &content[err_pos..];

            let pre_warnings = pre_error
                .lines()
                .filter(|l| is_compiler_warning_start(l))
                .count();

            if pre_warnings >= 3 {
                let mut condensed_pre = String::new();
                for line in pre_error.lines() {
                    if is_compilation_progress(line) || is_compiler_warning_start(line) {
                        continue;
                    }
                    let trimmed = line.trim_start();
                    if trimmed.starts_with("-->")
                        || trimmed.starts_with("=")
                        || trimmed.starts_with("|")
                    {
                        continue;
                    }
                    condensed_pre.push_str(line);
                    condensed_pre.push('\n');
                }

                let mut out = String::with_capacity(content.len());
                out.push_str(&format!(
                    "⚠ [{pre_warnings} compiler warnings omitted before first failure.{id_hint}]\n\n"
                ));
                out.push_str(condensed_pre.trim());
                if !condensed_pre.trim().is_empty() {
                    out.push_str("\n\n");
                }
                // 100% VERBATIM KEEP: Every error, panic, traceback, and exception details
                out.push_str(error_and_tail);
                return Some(out);
            }
        }
    }

    None
}

/// Streamline tool output in an append-only, prompt-cache friendly manner.
pub fn streamline_tool_output(
    content: &str,
    is_failure: bool,
    artifact_id: Option<&str>,
) -> String {
    let after_diff =
        fold_assertion_diff(content, artifact_id).unwrap_or_else(|| content.to_string());

    if let Some(sanitized) = sanitize_build_output(&after_diff, is_failure, artifact_id) {
        if sanitized.len() < after_diff.len() {
            return sanitized;
        }
    }

    after_diff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_rust_assertion_diff() {
        let mut left = String::new();
        let mut right = String::new();
        for i in 0..50 {
            let l = format!("line {i} instructions and long system prompt text\n");
            left.push_str(&l);
            right.push_str(&l);
        }
        left.push_str("\", cache_epoch: 1 }\n");
        right.push_str("\", cache_epoch: 0 }\n");

        let raw = format!(
            "thread 'test' panicked at lib.rs:10:5:\nassertion `left == right` failed\n  left: {left}\n right: {right}\nnote: run with `RUST_BACKTRACE=1`\n"
        );

        let folded = fold_assertion_diff(&raw, Some("art-rust")).expect("should fold");
        assert!(folded.contains("semantic diff: actual vs expected"));
        assert!(folded.contains("- \", cache_epoch: 1 }"));
        assert!(folded.contains("+ \", cache_epoch: 0 }"));
        assert!(folded.contains("lines of identical fields folded"));
        assert!(folded.len() < raw.len() / 2);
    }

    #[test]
    fn folds_java_junit_and_go_assertion_diff() {
        let mut exp = String::new();
        let mut act = String::new();
        for i in 0..50 {
            let l = format!("payload item {i} matching database row\n");
            exp.push_str(&l);
            act.push_str(&l);
        }
        exp.push_str("status: active\n");
        act.push_str("status: pending\n");

        let raw_junit = format!(
            "org.opentest4j.AssertionFailedError: expected: <\n  expected: {exp}\n  actual  : {act}\n\nStacktrace here"
        );

        let folded = fold_assertion_diff(&raw_junit, Some("art-junit")).expect("should fold junit");
        assert!(folded.contains("semantic diff: actual vs expected"));
        assert!(folded.contains("- status: active"));
        assert!(folded.contains("+ status: pending"));
        assert!(folded.contains("lines of identical fields folded"));
    }

    #[test]
    fn sanitizes_android_gradle_and_java_maven_build() {
        let mut raw = String::new();
        raw.push_str("> Task :app:preBuild UP-TO-DATE\n");
        raw.push_str("> Task :app:compileDebugAidl NO-SOURCE\n");
        raw.push_str("> Task :app:compileDebugKotlin\n");
        raw.push_str("> Task :app:compileDebugJavaWithJavac\n");
        for i in 0..15 {
            raw.push_str(&format!(
                "w: [deprecation] TaskA{i}.java uses deprecated API\n"
            ));
        }
        raw.push_str("BUILD SUCCESSFUL in 4s\n");
        raw.push_str("32 actionable tasks: 12 executed, 20 up-to-date\n");
        raw.push_str("[exit code 0]\n");

        let sanitized = sanitize_build_output(&raw, false, Some("art-android"))
            .expect("should sanitize gradle");
        assert!(sanitized.contains("[Compiled/Built 4 steps successfully]"));
        assert!(sanitized.contains("15 compiler warnings suppressed"));
        assert!(sanitized.contains("BUILD SUCCESSFUL"));
    }

    #[test]
    fn sanitizes_csharp_and_cpp_build_failure() {
        let mut raw = String::new();
        for i in 0..10 {
            raw.push_str(&format!(
                "C:\\Project\\File{i}.cs(15,10): warning CS0168: The variable 'e' is never used\n"
            ));
        }
        raw.push_str("C:\\Project\\Service.cs(42,12): error CS0246: The type or namespace name 'UserToken' could not be found\n");
        raw.push_str("Build FAILED.\n");
        raw.push_str("[exit code 1]\n");

        let sanitized = sanitize_build_output(&raw, true, Some("art-cs"))
            .expect("should sanitize csharp error");
        assert!(sanitized.contains("10 compiler warnings omitted before first failure"));
        assert!(sanitized.contains("error CS0246"));
        assert!(sanitized.contains("Build FAILED"));
    }

    #[test]
    fn preserves_python_traceback_verbatim() {
        let mut raw = String::new();
        for _ in 0..6 {
            raw.push_str("UserWarning: Pydantic deprecated config warning in model.py:10\n");
        }
        raw.push_str("Traceback (most recent call last):\n");
        raw.push_str("  File \"test_auth.py\", line 45, in test_login\n");
        raw.push_str("    assert token.is_valid is True\n");
        raw.push_str("AssertionError: assert False is True\n");
        raw.push_str("[exit code 1]\n");

        let sanitized =
            sanitize_build_output(&raw, true, Some("art-py")).expect("should sanitize python");
        assert!(sanitized.contains("compiler warnings omitted before first failure"));
        assert!(sanitized.contains("Traceback (most recent call last):"));
        assert!(sanitized.contains("AssertionError: assert False is True"));
    }

    #[test]
    fn filters_ssh_connection_noise_and_locates_generic_source_error() {
        let mut raw = String::new();
        raw.push_str(
            "Warning: Permanently added '10.0.0.5' (ED25519) to the list of known hosts.\n",
        );
        raw.push_str("Pseudo-terminal will not be allocated because stdin is not a terminal.\n");
        for i in 0..15 {
            raw.push_str(&format!("warning: unused import `M{i}` in file_{i}.rs\n"));
        }
        // Generic source line error without explicit 'error' keyword in dictionary
        raw.push_str("src/native/engine.c:88:14: syntax issue or broken macro\n");
        raw.push_str("[exit code 2]\n");

        let sanitized =
            sanitize_build_output(&raw, true, Some("art-ssh")).expect("should sanitize ssh output");
        assert!(sanitized.contains("compiler warnings omitted before first failure"));
        assert!(sanitized.contains("src/native/engine.c:88:14: syntax issue"));
        assert!(!sanitized.contains("Permanently added"));
    }
}
