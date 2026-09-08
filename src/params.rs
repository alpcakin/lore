//! Placeholder syntax for command definitions.
//!
//! A placeholder is written `<name>` or `<name:default>`. Names may contain
//! ASCII letters, digits, `_` and `-`. A literal angle bracket is escaped with a
//! preceding backslash.
//!
//! Angle brackets were chosen over `{{name}}` because Go template syntax occurs
//! constantly in the docker and kubectl commands this tool targets.

use std::collections::BTreeMap;
use std::ops::Range;

const ESCAPE: u8 = b'\\';
const OPEN: u8 = b'<';
const CLOSE: u8 = b'>';

/// One placeholder occurrence in a command string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub name: String,
    pub default: Option<String>,
    /// Byte range the placeholder occupies, including the angle brackets.
    pub span: Range<usize>,
}

/// Every placeholder in `cmd`, in the order it appears.
pub fn parse(cmd: &str) -> Vec<Placeholder> {
    let bytes = cmd.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            ESCAPE if bytes.get(i + 1) == Some(&OPEN) => i += 2,
            OPEN => match scan(cmd, i) {
                Some(placeholder) => {
                    i = placeholder.span.end;
                    found.push(placeholder);
                }
                None => i += 1,
            },
            _ => i += 1,
        }
    }

    found
}

/// Distinct placeholder names in `cmd`, in the order they first appear.
pub fn names(cmd: &str) -> Vec<String> {
    let mut seen = Vec::new();
    for placeholder in parse(cmd) {
        if !seen.contains(&placeholder.name) {
            seen.push(placeholder.name);
        }
    }
    seen
}

/// Substitutes placeholder values and resolves escaped angle brackets.
///
/// A placeholder missing from `values` falls back to its inline default, then to
/// an empty string.
pub fn render(cmd: &str, values: &BTreeMap<String, String>) -> String {
    let bytes = cmd.as_bytes();
    let mut out = String::with_capacity(cmd.len());
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            ESCAPE if bytes.get(i + 1) == Some(&OPEN) => {
                out.push(OPEN as char);
                i += 2;
            }
            OPEN => match scan(cmd, i) {
                Some(placeholder) => {
                    let value = values
                        .get(&placeholder.name)
                        .or(placeholder.default.as_ref())
                        .map(String::as_str)
                        .unwrap_or_default();
                    out.push_str(value);
                    i = placeholder.span.end;
                }
                None => {
                    out.push(OPEN as char);
                    i += 1;
                }
            },
            ESCAPE => {
                out.push(ESCAPE as char);
                i += 1;
            }
            _ => {
                let start = i;
                i += 1;
                while i < bytes.len() && !matches!(bytes[i], OPEN | ESCAPE) {
                    i += 1;
                }
                out.push_str(&cmd[start..i]);
            }
        }
    }

    out
}

/// Reads one placeholder starting at the opening bracket at `start`.
fn scan(cmd: &str, start: usize) -> Option<Placeholder> {
    let bytes = cmd.as_bytes();
    let name_start = start + 1;
    let mut i = name_start;

    while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-')) {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    let name = cmd[name_start..i].to_string();

    let default = if bytes.get(i) == Some(&b':') {
        let value_start = i + 1;
        while i < bytes.len() && bytes[i] != CLOSE {
            i += 1;
        }
        Some(cmd[value_start..i].to_string())
    } else {
        None
    };

    if bytes.get(i) != Some(&CLOSE) {
        return None;
    }

    Some(Placeholder {
        name,
        default,
        span: start..i + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(cmd: &str) -> Vec<(String, Option<String>)> {
        parse(cmd)
            .into_iter()
            .map(|p| (p.name, p.default))
            .collect()
    }

    #[test]
    fn reads_a_bare_placeholder() {
        assert_eq!(
            parsed("docker logs <container>"),
            [("container".to_string(), None)]
        );
    }

    #[test]
    fn reads_an_inline_default() {
        assert_eq!(
            parsed("kubectl get pods -n <namespace:default>"),
            [("namespace".to_string(), Some("default".to_string()))]
        );
    }

    #[test]
    fn reads_hyphenated_names_in_order() {
        assert_eq!(
            parsed("kubectl port-forward svc/<service> <local-port>:<remote-port>"),
            [
                ("service".to_string(), None),
                ("local-port".to_string(), None),
                ("remote-port".to_string(), None),
            ]
        );
    }

    #[test]
    fn ignores_go_template_syntax() {
        let cmd = r#"docker ps --format "table {{.Names}}\t{{.Status}}""#;
        assert!(parse(cmd).is_empty());
    }

    #[test]
    fn ignores_shell_redirection() {
        assert!(parse("mysql -u root < dump.sql").is_empty());
        assert!(parse("make 2>&1 | tee log").is_empty());
        assert!(parse("cat <file.txt").is_empty());
    }

    #[test]
    fn escaped_bracket_is_not_a_placeholder() {
        let cmd = r"echo \<literal>";
        assert!(parse(cmd).is_empty());
        assert_eq!(render(cmd, &BTreeMap::new()), "echo <literal>");
    }

    #[test]
    fn render_substitutes_values_over_defaults() {
        let values = BTreeMap::from([("namespace".to_string(), "prod".to_string())]);
        assert_eq!(
            render("kubectl get pods -n <namespace:default>", &values),
            "kubectl get pods -n prod"
        );
    }

    #[test]
    fn render_falls_back_to_the_default() {
        assert_eq!(
            render("kubectl get pods -n <namespace:default>", &BTreeMap::new()),
            "kubectl get pods -n default"
        );
    }

    #[test]
    fn render_leaves_go_templates_untouched() {
        let cmd = r"docker inspect -f '{{.Id}}' <container>";
        let values = BTreeMap::from([("container".to_string(), "web".to_string())]);
        assert_eq!(render(cmd, &values), r"docker inspect -f '{{.Id}}' web");
    }

    #[test]
    fn render_preserves_multibyte_text() {
        let values = BTreeMap::from([("dir".to_string(), "günlük".to_string())]);
        assert_eq!(
            render("echo 'ölçüm →' && ls <dir>", &values),
            "echo 'ölçüm →' && ls günlük"
        );
    }

    #[test]
    fn parse_handles_multibyte_text_before_a_placeholder() {
        assert_eq!(parsed("echo 'ölçüm' <path>"), [("path".to_string(), None)]);
    }

    #[test]
    fn lone_backslash_is_preserved() {
        assert_eq!(
            render(r"copy C:\src\file <dst>", &BTreeMap::new()),
            r"copy C:\src\file "
        );
    }

    #[test]
    fn names_are_deduplicated_in_first_appearance_order() {
        assert_eq!(
            names("cp <src> <dst> && diff <src> <dst>"),
            ["src".to_string(), "dst".to_string()]
        );
    }
}
