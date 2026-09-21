//! `bash_send_keys` — inject keystrokes into a live bash task by `bashid`.
//!
//! Mapping is VT/ASCII (platform-independent). `Ctrl+C` also delivers SIGINT
//! to the process group on Unix; Windows gets ETX on the stdin pipe.

use super::bash_runtime::{find_live_bash, interrupt_by_id, send_stdin};
use super::{err, ok};
use async_trait::async_trait;
use jeikcode_kernel::tool::{RiskLevel, Tool, ToolContext, ToolResult};
use serde::Deserialize;
use serde_json::json;

#[derive(Default)]
pub struct BashSendKeysTool;

#[derive(Deserialize)]
struct SendKeysArgs {
    bashid: String,
    #[serde(deserialize_with = "deserialize_keys")]
    keys: Vec<String>,
}

fn deserialize_keys<'de, D>(d: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = serde_json::Value::deserialize(d)?;
    match v {
        serde_json::Value::String(s) => Ok(vec![s]),
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|i| match i {
                serde_json::Value::String(s) => Ok(s),
                other => Ok(other.to_string()),
            })
            .collect(),
        other => Ok(vec![other.to_string()]),
    }
}

#[async_trait]
impl Tool for BashSendKeysTool {
    fn name(&self) -> &str {
        "bash_send_keys"
    }
    fn description(&self) -> &str {
        "Type keys into a live shell identified by `bashid` (interactive prompts, REPL, confirmations). \
         Use natural names: letters, `Enter`, `Tab`, `Esc`, `Up`/`Down`/`Left`/`Right`, `Ctrl+C`, `Ctrl+D`, `Alt+X`. \
         Combinations use `+`. Multiple tokens are sent in order (`y Enter`). \
         Does not change the runtime timeout budget."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "bashid": {
                    "type": "string",
                    "description": "The bashid of a running shell (from run_command background / auto-detach)."
                },
                "keys": {
                    "description": "One string (`Ctrl+C`, `y Enter`) or an array of tokens.",
                    "oneOf": [
                        { "type": "string" },
                        { "type": "array", "items": { "type": "string" } }
                    ]
                }
            },
            "required": ["bashid", "keys"]
        })
    }
    fn risk(&self, _args: &str) -> RiskLevel {
        RiskLevel::Safe
    }
    fn parallel_safe(&self, _args: &str) -> bool {
        false
    }
    async fn execute(&self, args: &str, _ctx: &ToolContext) -> ToolResult {
        let a: SendKeysArgs = match serde_json::from_str(args) {
            Ok(a) => a,
            Err(e) => {
                return err(format!(
                    "bash_send_keys: invalid arguments: {e}. \
                     Expected {{\"bashid\":\"b-…\",\"keys\":\"Enter\"}}."
                ))
            }
        };
        let id = a.bashid.trim();
        if id.is_empty() {
            return err("bash_send_keys: bashid must not be empty");
        }
        if find_live_bash(id).is_none() {
            return err(format!(
                "bash_send_keys: no live bash with bashid `{id}`. It may have already exited."
            ));
        }
        let joined = a.keys.join(" ");
        let bytes = match encode_keys(&joined) {
            Ok(b) => b,
            Err(e) => return err(format!("bash_send_keys: {e}")),
        };
        if bytes.is_empty() {
            return err("bash_send_keys: keys produced no bytes");
        }
        let wants_int = bytes.contains(&0x03);
        match send_stdin(id, bytes) {
            Ok(()) => {
                if wants_int {
                    interrupt_by_id(id);
                }
                let tail = find_live_bash(id)
                    .map(|l| l.tail_logs(8).join("\n"))
                    .unwrap_or_default();
                let mut out = format!("sent keys to `{id}` ({joined}).");
                if !tail.trim().is_empty() {
                    out.push_str("\n\nRecent output:\n");
                    out.push_str(&tail);
                }
                ok(out)
            }
            Err(e) => err(format!("bash_send_keys: {e}")),
        }
    }
}

/// Encode a natural key spec into terminal bytes (VT/ASCII).
pub fn encode_keys(spec: &str) -> Result<Vec<u8>, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Ok(Vec::new());
    }
    if !looks_like_key_spec(spec) {
        return Ok(spec.as_bytes().to_vec());
    }
    let mut out = Vec::new();
    for token in tokenize_key_spec(spec) {
        out.extend(encode_token(&token)?);
    }
    Ok(out)
}

fn looks_like_key_spec(s: &str) -> bool {
    s.contains('+')
        || s.split_whitespace().any(|t| {
            let l = t
                .trim_matches(|c| c == '"' || c == '\'')
                .to_ascii_lowercase();
            is_named_key(&l)
                || l.starts_with("ctrl")
                || l.starts_with("alt")
                || l.starts_with("shift")
        })
}

fn tokenize_key_spec(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = in_q {
            if c == q {
                in_q = None;
            } else {
                cur.push(c);
            }
            continue;
        }
        if c == '"' || c == '\'' {
            in_q = Some(c);
            continue;
        }
        if c.is_whitespace() {
            if !cur.is_empty() {
                out.push(std::mem::take(&mut cur));
            }
            continue;
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn is_named_key(l: &str) -> bool {
    matches!(
        l,
        "enter"
            | "return"
            | "ret"
            | "tab"
            | "esc"
            | "escape"
            | "space"
            | "spc"
            | "backspace"
            | "bs"
            | "delete"
            | "del"
            | "up"
            | "down"
            | "left"
            | "right"
            | "home"
            | "end"
            | "pageup"
            | "pgup"
            | "pagedown"
            | "pgdn"
            | "plus"
    ) || (l.starts_with('f') && l[1..].chars().all(|c| c.is_ascii_digit()) && l.len() <= 3)
}

fn encode_token(token: &str) -> Result<Vec<u8>, String> {
    let parts: Vec<String> = token.split('+').map(|p| p.to_ascii_lowercase()).collect();
    let mut ctrl = false;
    let mut alt = false;
    let mut shift = false;
    let mut key = String::new();
    for p in &parts {
        match p.as_str() {
            "ctrl" | "control" | "ctl" => ctrl = true,
            "alt" | "meta" | "option" | "opt" => alt = true,
            "shift" => shift = true,
            other => key = other.to_string(),
        }
    }
    if key.is_empty() {
        return Err(format!("empty key in `{token}`"));
    }
    let mut bytes = named_or_char(&key, shift)?;
    if ctrl {
        if bytes.len() == 1 && bytes[0].is_ascii_alphabetic() {
            bytes[0] = bytes[0].to_ascii_uppercase() & 0x1f;
        } else if bytes == b"[" {
            bytes = vec![0x1b];
        } else if bytes.len() == 1 && (bytes[0] as char).is_ascii() {
            bytes[0] &= 0x1f;
        }
    }
    if alt {
        let mut v = vec![0x1b];
        v.extend(bytes);
        bytes = v;
    }
    Ok(bytes)
}

fn named_or_char(key: &str, shift: bool) -> Result<Vec<u8>, String> {
    let l = key.to_ascii_lowercase();
    Ok(match l.as_str() {
        "enter" | "return" | "ret" => b"\n".to_vec(),
        "tab" => b"\t".to_vec(),
        "esc" | "escape" => vec![0x1b],
        "space" | "spc" => b" ".to_vec(),
        "backspace" | "bs" => vec![0x7f],
        "delete" | "del" => b"\x1b[3~".to_vec(),
        "up" => b"\x1b[A".to_vec(),
        "down" => b"\x1b[B".to_vec(),
        "right" => b"\x1b[C".to_vec(),
        "left" => b"\x1b[D".to_vec(),
        "home" => b"\x1b[H".to_vec(),
        "end" => b"\x1b[F".to_vec(),
        "pageup" | "pgup" => b"\x1b[5~".to_vec(),
        "pagedown" | "pgdn" => b"\x1b[6~".to_vec(),
        "plus" => b"+".to_vec(),
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            let n: u8 = f[1..].parse().unwrap();
            if !(1..=12).contains(&n) {
                return Err(format!("unsupported function key `{key}`"));
            }
            // xterm: F1-F4 SS3, F5+ CSI
            match n {
                1 => b"\x1bOP".to_vec(),
                2 => b"\x1bOQ".to_vec(),
                3 => b"\x1bOR".to_vec(),
                4 => b"\x1bOS".to_vec(),
                5 => b"\x1b[15~".to_vec(),
                6 => b"\x1b[17~".to_vec(),
                7 => b"\x1b[18~".to_vec(),
                8 => b"\x1b[19~".to_vec(),
                9 => b"\x1b[20~".to_vec(),
                10 => b"\x1b[21~".to_vec(),
                11 => b"\x1b[23~".to_vec(),
                12 => b"\x1b[24~".to_vec(),
                _ => unreachable!(),
            }
        }
        _ => {
            let mut s = key.to_string();
            if shift {
                s = s.to_uppercase();
            }
            if s.chars().count() != 1 && !s.is_ascii() {
                // allow utf-8 literal token
            }
            s.into_bytes()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_text_without_named_keys() {
        assert_eq!(encode_keys("hello").unwrap(), b"hello");
    }

    #[test]
    fn enter_and_letter() {
        assert_eq!(encode_keys("y Enter").unwrap(), b"y\n");
    }

    #[test]
    fn ctrl_c_is_etx() {
        assert_eq!(encode_keys("Ctrl+C").unwrap(), vec![0x03]);
        assert_eq!(encode_keys("ctrl+c").unwrap(), vec![0x03]);
    }

    #[test]
    fn ctrl_d_eof() {
        assert_eq!(encode_keys("Ctrl+D").unwrap(), vec![0x04]);
    }

    #[test]
    fn arrows_are_csi() {
        assert_eq!(encode_keys("Up").unwrap(), b"\x1b[A");
    }

    #[test]
    fn quoted_literal_keeps_spaces() {
        assert_eq!(
            encode_keys("\"hello world\" Enter").unwrap(),
            b"hello world\n"
        );
    }

    #[test]
    fn does_not_eat_python3_config_style() {
        // sanity: named-key detector does not fire on ordinary sentences
        assert_eq!(encode_keys("print(1)").unwrap(), b"print(1)");
    }
}
