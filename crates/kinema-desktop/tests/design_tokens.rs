//! Source scan enforcing the design-token and layer rules (see
//! `docs/design/design-system.md`). Std only.
//!
//! Scans `src/ui/{primitives,blocks,views,motion}`; raw style literals belong in
//! `ui/theme`. A line may opt out with a same-line `// token-exempt: <reason>`.

#![allow(clippy::expect_used)]

use std::fs;
use std::path::{Path, PathBuf};

const SCANNED: &[&str] = &["primitives", "blocks", "views", "motion"];
const EXEMPT_MARK: &str = "// token-exempt:";

#[derive(Debug, PartialEq, Eq)]
struct Violation {
    line: usize,
    what: String,
}

/// Replaces comments and string/char literal contents with spaces (newlines
/// kept), so later matching and brace counting ignore them.
fn mask(src: &str) -> String {
    let b: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let n = b.get(i + 1).copied().unwrap_or('\0');
        if c == '/' && n == '/' {
            while i < b.len() && b[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && n == '*' {
            let mut depth = 0;
            while i < b.len() {
                if b[i] == '/' && b.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if b[i] == '*' && b.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(b[i]));
                    i += 1;
                }
            }
        } else if c == 'r' && (n == '"' || n == '#') && !prev_ident(&b, i) {
            let mut j = i + 1;
            let mut hashes = 0;
            while b.get(j) == Some(&'#') {
                hashes += 1;
                j += 1;
            }
            if b.get(j) != Some(&'"') {
                out.push(c);
                i += 1;
                continue;
            }
            j += 1;
            let close: Vec<char> = std::iter::once('"')
                .chain(std::iter::repeat_n('#', hashes))
                .collect();
            while j < b.len() && b[j..].len() >= close.len() && b[j..j + close.len()] != close[..] {
                j += 1;
            }
            let end = (j + close.len()).min(b.len());
            out.extend(b[i..end].iter().map(|&c| blank(c)));
            i = end;
        } else if c == '"' {
            out.push('"');
            i += 1;
            while i < b.len() && b[i] != '"' {
                if b[i] == '\\' && i + 1 < b.len() {
                    out.push(' ');
                    i += 1;
                }
                out.push(blank(b[i]));
                i += 1;
            }
            if i < b.len() {
                out.push('"');
                i += 1;
            }
        } else if c == '\'' {
            // Char literal vs lifetime.
            let end = if n == '\\' {
                (i + 2..b.len().min(i + 12)).find(|&k| b[k] == '\'')
            } else if b.get(i + 2) == Some(&'\'') {
                Some(i + 2)
            } else {
                None
            };
            match end {
                Some(e) => {
                    out.push('\'');
                    out.extend(std::iter::repeat_n(' ', e - i - 1));
                    out.push('\'');
                    i = e + 1;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn prev_ident(b: &[char], i: usize) -> bool {
    i > 0 && (b[i - 1].is_alphanumeric() || b[i - 1] == '_')
}

/// Blanks every item annotated with a `#[cfg(...)]` that mentions `test`.
/// Input must already be masked.
fn strip_test_items(masked: &str) -> String {
    let mut b: Vec<char> = masked.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '#' && b.get(i + 1) == Some(&'[') {
            let start = i;
            let Some(attr_end) = match_close(&b, i + 1, '[', ']') else {
                break;
            };
            let attr: String = b[i + 2..attr_end].iter().collect();
            let attr = attr.trim();
            let is_test_cfg = attr
                .strip_prefix("cfg")
                .is_some_and(|r| r.trim_start().starts_with('(') && words(r).any(|w| w == "test"));
            i = attr_end + 1;
            if !is_test_cfg {
                continue;
            }
            // Item extends to `;` or a matched `{}` block at paren depth 0.
            let mut j = i;
            let mut depth = 0i32;
            let end = loop {
                match b.get(j) {
                    None => break b.len(),
                    Some('(' | '[') => depth += 1,
                    Some(')' | ']') => depth -= 1,
                    Some(';') if depth <= 0 => break j + 1,
                    Some('{') if depth <= 0 => {
                        break match_close(&b, j, '{', '}').map_or(b.len(), |e| e + 1);
                    }
                    _ => {}
                }
                j += 1;
            };
            for c in &mut b[start..end] {
                if *c != '\n' {
                    *c = ' ';
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    b.into_iter().collect()
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
}

fn match_close(b: &[char], open_at: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 0;
    for (k, &c) in b.iter().enumerate().skip(open_at) {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

fn exempt(raw_line: &str) -> bool {
    raw_line
        .find(EXEMPT_MARK)
        .is_some_and(|p| !raw_line[p + EXEMPT_MARK.len()..].trim().is_empty())
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Byte offsets of `needle` in `line` not preceded by an identifier char.
fn find_bounded<'a>(line: &'a str, needle: &'a str) -> impl Iterator<Item = usize> + 'a {
    line.match_indices(needle)
        .map(|(p, _)| p)
        .filter(move |&p| !line[..p].chars().next_back().is_some_and(is_ident))
}

fn starts_digit(s: &str) -> bool {
    s.chars().next().is_some_and(|c| c.is_ascii_digit())
}

const TEXT_HELPERS: &[&str] = &[
    "text_xs",
    "text_sm",
    "text_base",
    "text_lg",
    "text_xl",
    "text_2xl",
    "rounded_sm",
    "rounded_md",
    "rounded_lg",
    "rounded_xl",
    "rounded_full",
];
const NUMERIC_PREFIXES: &[&str] = &[
    "p", "px", "py", "pt", "pb", "pl", "pr", "m", "mx", "my", "mt", "mb", "ml", "mr", "gap",
    "gap_x", "gap_y", "w", "h", "size",
];

fn style_violations(line: &str, out: &mut Vec<String>) {
    for p in find_bounded(line, "px(") {
        if starts_digit(&line[p + 3..]) {
            out.push("px(<digit>".into());
        }
    }
    for pat in ["rgb(", "rgba(", "hsla("] {
        if find_bounded(line, pat).next().is_some() {
            out.push(pat.into());
        }
    }
    for pat in ["Hsla {", "Hsla{"] {
        for (p, _) in line.match_indices(pat) {
            // Struct literals only; skip `-> Hsla {` and `impl [X for] Hsla {`.
            let before = line[..p].trim_end();
            let prev = before.rsplit(|c: char| !is_ident(c)).next().unwrap_or("");
            if before.ends_with("->") || matches!(prev, "impl" | "for" | "struct" | "enum") {
                continue;
            }
            out.push("Hsla {".into());
        }
    }
    for (p, _) in line.match_indices(".opacity(") {
        if starts_digit(&line[p + 9..]) {
            out.push(".opacity(<digit>".into());
        }
    }
    for f in ["from_millis(", "from_secs(", "from_secs_f32("] {
        let pat = format!("Duration::{f}");
        for (p, _) in line.match_indices(&pat) {
            if starts_digit(&line[p + pat.len()..]) {
                out.push(format!("Duration::{f}<digit>"));
            }
        }
    }
    for m in [".text_size(", ".font_weight("] {
        for (p, _) in line.match_indices(m) {
            let rest = &line[p + m.len()..];
            if starts_digit(rest)
                || ["FontWeight::", "rems(", "px("]
                    .iter()
                    .any(|l| rest.starts_with(l))
            {
                out.push(format!("{m}<literal>"));
            }
        }
    }
    for (p, _) in line.match_indices('.') {
        let rest = &line[p + 1..];
        let Some(end) = rest.find('(') else { continue };
        let name = &rest[..end];
        if name.is_empty() || !name.chars().all(is_ident) {
            continue;
        }
        let numeric = NUMERIC_PREFIXES.iter().any(|pre| {
            name.strip_prefix(pre)
                .and_then(|r| r.strip_prefix('_'))
                .is_some_and(starts_digit)
        });
        if TEXT_HELPERS.contains(&name) || numeric {
            out.push(format!(".{name}()"));
        }
    }
    for pat in ["white()", "black()"] {
        if find_bounded(line, pat).next().is_some() {
            out.push(pat.into());
        }
    }
}

/// `rel` is the path below `src/ui`, with `/` separators.
fn scan_source(rel: &str, src: &str) -> (Vec<Violation>, usize) {
    let code = strip_test_items(&mask(src));
    let in_motion = rel.starts_with("motion/");
    let in_prim = rel.starts_with("primitives/");
    let in_blocks = rel.starts_with("blocks/");
    let mut found = Vec::new();
    let mut exemptions = 0;
    for (idx, (raw, line)) in src.lines().zip(code.lines()).enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut hits = Vec::new();
        style_violations(line, &mut hits);
        if !in_motion {
            for pat in [
                "Animation::new(",
                "with_animation(",
                "with_spring(",
                "SpringConfig::new(",
            ] {
                if line.contains(pat) {
                    hits.push(pat.into());
                }
            }
        }
        if in_prim || in_blocks {
            for pat in ["crate::controllers", "ui::views"] {
                if line.contains(pat) {
                    hits.push(format!("layer: {pat}"));
                }
            }
        }
        if in_prim {
            for pat in ["crate::ui::blocks", "super::super::blocks"] {
                if line.contains(pat) {
                    hits.push(format!("layer: {pat}"));
                }
            }
            let ws: Vec<&str> = words(line).collect();
            for pair in ws.windows(2) {
                if matches!(pair[0], "struct" | "enum" | "trait")
                    && ["Poster", "Episode", "Stream", "Watched", "Library"]
                        .iter()
                        .any(|d| pair[1].contains(d))
                {
                    hits.push(format!("layer: domain type name `{}`", pair[1]));
                }
            }
        }
        if hits.is_empty() {
            continue;
        }
        if exempt(raw) {
            exemptions += 1;
            continue;
        }
        for what in hits {
            found.push(Violation {
                line: idx + 1,
                what,
            });
        }
    }
    (found, exemptions)
}

fn skip_file(name: &str) -> bool {
    name == "tests.rs" || name == "fixtures.rs" || name.ends_with("_tests.rs")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    entries.sort();
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if p.is_dir() {
            if name != "tests" {
                walk(&p, out);
            }
        } else if name.ends_with(".rs") && !skip_file(&name) {
            out.push(p);
        }
    }
}

#[test]
fn ui_sources_follow_design_token_and_layer_rules() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    let mut files = Vec::new();
    for d in SCANNED {
        walk(&ui.join(d), &mut files);
    }
    assert!(!files.is_empty(), "scan found no files");
    let mut report = Vec::new();
    let mut exemptions = 0;
    for f in &files {
        let rel = f
            .strip_prefix(&ui)
            .expect("walked below ui")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        let src = fs::read_to_string(f).expect("readable source");
        let (v, e) = scan_source(&rel, &src);
        exemptions += e;
        report.extend(
            v.into_iter()
                .map(|v| format!("{}:{}: {}", f.display(), v.line, v.what)),
        );
    }
    println!(
        "token-exempt lines: {exemptions} ({} files scanned)",
        files.len()
    );
    assert!(
        report.is_empty(),
        "{} design-token/layer violation(s) ({exemptions} exemptions):\n{}",
        report.len(),
        report.join("\n")
    );
}

#[cfg(test)]
mod scanner {
    use super::*;

    fn whats(rel: &str, src: &str) -> Vec<String> {
        scan_source(rel, src)
            .0
            .into_iter()
            .map(|v| v.what)
            .collect()
    }

    #[test]
    fn negative_control_catches_violations() {
        let src = "fn f() {\n    div().p_2().w(px(12.0)).bg(rgb(0xffffff)).text_sm();\n}\n";
        let v = scan_source("blocks/a.rs", src).0;
        assert!(v.iter().all(|v| v.line == 2));
        let w: Vec<_> = v.iter().map(|v| v.what.as_str()).collect();
        for want in ["px(<digit>", "rgb(", ".p_2()", ".text_sm()"] {
            assert!(w.contains(&want), "missing {want}: {w:?}");
        }
        assert!(whats("views/a.rs", "let d = Duration::from_millis(200);").len() == 1);
        assert!(
            whats(
                "views/a.rs",
                "x.opacity(0.5).text_size(14.0).font_weight(700)"
            )
            .len()
                == 3
        );
        assert!(whats("views/a.rs", "let c = white(); let d = black();").len() == 2);
        assert_eq!(
            whats("views/a.rs", "let c = Hsla { h: 0., s: 0., l: 0., a: 1. };").len(),
            1
        );
        assert!(whats("views/a.rs", "fn f() -> Hsla {\n}\nimpl Foo for Hsla {").is_empty());
        assert!(whats("views/a.rs", "d.gap_x_3().size_4().mt_1()").len() == 3);
    }

    #[test]
    fn non_numeric_helpers_pass() {
        assert!(whats("views/a.rs", "d.w_full().h_full().size_full().p(space::MD)").is_empty());
    }

    #[test]
    fn test_modules_and_items_are_stripped() {
        let src = "#[cfg(test)]\nmod tests {\n    fn t() { px(3.0); }\n}\n\
                   #[cfg(all(test, unix))]\nfn u() { rgb(1) }\n\
                   #[cfg(any(test, debug_assertions))] const X: f32 = px(1.0);\n\
                   fn real() { px(4.0); }\n";
        let v = scan_source("views/a.rs", src).0;
        assert_eq!(
            v,
            vec![Violation {
                line: 8,
                what: "px(<digit>".into()
            }]
        );
    }

    #[test]
    fn exemption_requires_reason() {
        let ok = "let a = px(1.0); // token-exempt: hairline\n";
        let (v, e) = scan_source("views/a.rs", ok);
        assert!(v.is_empty());
        assert_eq!(e, 1);
        assert_eq!(
            whats("views/a.rs", "let a = px(1.0); // token-exempt:   \n").len(),
            1
        );
    }

    #[test]
    fn braces_in_strings_and_comments_do_not_confuse_stripper() {
        let src = "#[cfg(test)]\nmod tests {\n    const S: &str = \"}}} {\";\n    const C: char = '}';\n    \
                   // } }\n    /* } */\n    const R: &str = r#\"} \"quoted\" }\"#;\n    fn t() { px(9.0); }\n}\n\
                   fn real() { let s = \"{ px(5.0) }\"; px(6.0); }\n";
        let v = scan_source("views/a.rs", src).0;
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].line, 10);
    }

    #[test]
    fn comments_are_ignored() {
        assert!(whats("views/a.rs", "// px(3.0) rgb(1)\n/// white()\n/* px(2) */").is_empty());
    }

    #[test]
    fn animation_api_only_in_motion() {
        assert!(whats("motion/a.rs", "Animation::new(d)").is_empty());
        assert_eq!(whats("blocks/a.rs", "Animation::new(d)").len(), 1);
        assert_eq!(whats("views/a.rs", "x.with_animation(a)").len(), 1);
    }

    #[test]
    fn layer_rules() {
        assert_eq!(
            whats("primitives/a.rs", "use crate::controllers::X;").len(),
            1
        );
        assert_eq!(whats("blocks/a.rs", "use crate::ui::views::X;").len(), 1);
        assert_eq!(
            whats("primitives/a.rs", "use crate::ui::blocks::X;").len(),
            1
        );
        assert!(whats("blocks/a.rs", "use crate::ui::primitives::X;").is_empty());
        assert!(whats("views/a.rs", "use crate::controllers::X;").is_empty());
        assert_eq!(whats("primitives/a.rs", "pub struct PosterCard;").len(), 1);
        assert_eq!(whats("primitives/a.rs", "pub enum StreamKind {}").len(), 1);
        assert!(whats("blocks/a.rs", "pub struct PosterCard;").is_empty());
    }
}
