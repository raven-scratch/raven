//! `raven fmt`: canonical indentation and blank lines.
//!
//! This is a *line* formatter, not a pretty-printer. It re-indents each line
//! from the brace depth, trims trailing whitespace and collapses runs of blank
//! lines, and it changes nothing else — no reflowing, no reordering, no dropping
//! of comments.
//!
//! That restraint is deliberate. A formatter that rebuilt the source from the
//! AST would delete every `//` comment, because comments are not part of the
//! syntax; deleting a program's comments is not a formatting decision. A
//! line-based formatter cannot lose anything it did not understand.

use std::path::{Path, PathBuf};

use raven_scratch::diag::{Error, Result};

use crate::identity;

/// Format the given paths, or the whole project when none are given.
pub fn run(paths: &[PathBuf], check: bool) -> Result<()> {
    let files = if paths.is_empty() {
        project_files()?
    } else {
        collect(paths)?
    };
    if files.is_empty() {
        return Err(Error::msg("no `.rav` files to format")
            .note(format!("looked in `src/` next to `{}`", identity::MANIFEST)));
    }
    let mut changed = 0;
    for path in &files {
        let original = std::fs::read_to_string(path)?;
        let formatted = format_source(&original);
        if formatted == original {
            continue;
        }
        changed += 1;
        if check {
            println!("    Would reformat {}", path.display());
        } else {
            std::fs::write(path, &formatted)?;
            println!("    Formatted {}", path.display());
        }
    }
    if check && changed > 0 {
        return Err(Error::msg(format!("{changed} file(s) are not formatted")));
    }
    if changed == 0 {
        println!("    {} file(s) already formatted", files.len());
    }
    Ok(())
}

/// Canonical form of one file's text.
#[must_use]
pub fn format_source(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth: usize = 0;
    let mut blank_run = 0usize;
    let mut in_block_comment = false;

    for raw in text.lines() {
        let trimmed = raw.trim_end();
        if trimmed.trim().is_empty() {
            blank_run += 1;
            // At most one blank line in a row, and never at the very start.
            if blank_run == 1 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blank_run = 0;
        // A line that starts inside a block comment is left exactly as it is:
        // its layout is the author's, and its braces are not code.
        let starts_in_comment = in_block_comment;
        let scan = scan_line(trimmed, &mut in_block_comment);
        if starts_in_comment {
            out.push_str(trimmed);
            out.push('\n');
            continue;
        }
        let level = depth.saturating_sub(scan.leading_closes);
        for _ in 0..level {
            out.push_str("    ");
        }
        out.push_str(trimmed.trim_start());
        out.push('\n');
        depth += scan.opens;
        depth = depth.saturating_sub(scan.closes);
    }
    out
}

#[derive(Default)]
struct Scan {
    opens: usize,
    closes: usize,
    /// How many `}` characters the line begins with, so `}` lands one level out.
    leading_closes: usize,
}

fn scan_line(line: &str, in_block_comment: &mut bool) -> Scan {
    let mut scan = Scan::default();
    let mut chars = line.chars().peekable();
    let mut seen_token = false;
    while let Some(c) = chars.next() {
        if *in_block_comment {
            if c == '*' && chars.peek() == Some(&'/') {
                chars.next();
                *in_block_comment = false;
            }
            continue;
        }
        match c {
            '"' => {
                seen_token = true;
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            chars.next();
                        }
                        '"' => break,
                        _ => {}
                    }
                }
            }
            '/' if chars.peek() == Some(&'/') => break,
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                *in_block_comment = true;
            }
            '{' => {
                seen_token = true;
                scan.opens += 1;
            }
            '}' => {
                scan.closes += 1;
                if !seen_token {
                    scan.leading_closes += 1;
                }
            }
            c if !c.is_whitespace() => seen_token = true,
            _ => {}
        }
    }
    scan
}

/// Every `.rav` file under the project's `src/`.
fn project_files() -> Result<Vec<PathBuf>> {
    let manifest = PathBuf::from(identity::MANIFEST);
    let root = if manifest.exists() {
        PathBuf::from(".")
    } else {
        return Err(Error::msg(format!("no {} here", identity::MANIFEST))
            .note("pass file or directory paths, or run this from the project root"));
    };
    collect(&[root.join("src")])
}

fn collect(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            walk(path, &mut out)?;
        } else if path
            .extension()
            .is_some_and(|e| e == identity::SOURCE_EXTENSION)
        {
            out.push(path.clone());
        }
    }
    out.sort();
    out.dedup();
    Ok(out)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|e| e == identity::SOURCE_EXTENSION)
        {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indentation_follows_the_brace_depth() {
        let input = "sprite \"A\" {\n    on flag_clicked {\n  looks::say(\"hi\");\n}\n}\n";
        let expected =
            "sprite \"A\" {\n    on flag_clicked {\n        looks::say(\"hi\");\n    }\n}\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn else_lands_on_the_closing_line() {
        let input = "if a {\nb;\n} else {\nc;\n}\n";
        let expected = "if a {\n    b;\n} else {\n    c;\n}\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn comments_and_strings_are_left_alone() {
        let input = "// {\nlet a = \"}{ not a brace\"; // }\n";
        let expected = "// {\nlet a = \"}{ not a brace\"; // }\n";
        assert_eq!(format_source(input), expected);
    }

    #[test]
    fn block_comments_hide_their_braces() {
        let input = "/* {\n    still inside\n} */\nlet a = 1;\n";
        assert_eq!(format_source(input), input);
    }

    #[test]
    fn blank_runs_collapse_to_one() {
        let input = "a;\n\n\n\nb;\n";
        assert_eq!(format_source(input), "a;\n\nb;\n");
    }

    #[test]
    fn formatting_is_idempotent() {
        let input = "sprite \"A\" {\n on flag_clicked {\n looks::say(\"x\");\n }\n}\n";
        let once = format_source(input);
        assert_eq!(format_source(&once), once);
    }

    #[test]
    fn trailing_whitespace_is_removed() {
        assert_eq!(format_source("a;   \n"), "a;\n");
    }
}
