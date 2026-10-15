//! Enforces the directory shape and the size limits from AGENTS.md.

use std::fs;
use std::path::{Path, PathBuf};

const SRC_FILE_LIMIT: usize = 300;
const TEST_FILE_LIMIT: usize = 400;
const FUNCTION_LIMIT: usize = 60;

fn crates_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn entries(dir: &Path) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let mut files = Vec::new();
    let mut dirs = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            dirs.push(path);
        } else {
            files.push(path);
        }
    }
    (files, dirs)
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let (files, dirs) = entries(dir);
    out.extend(files.into_iter().filter(|f| f.extension().is_some_and(|e| e == "rs")));
    for d in dirs {
        rust_files(&d, out);
    }
}

fn code_lines(text: &str) -> usize {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("//"))
        .count()
}

/// The braces of one line that are code: not in strings, chars or comments.
/// Carries the open-string state over lines.
fn braces(line: &str, in_string: &mut bool) -> i32 {
    let chars: Vec<char> = line.chars().collect();
    let mut depth = 0;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if *in_string {
            match c {
                '\\' => i += 1,
                '"' => *in_string = false,
                _ => {}
            }
        } else {
            match c {
                '/' if chars.get(i + 1) == Some(&'/') => break,
                '"' => *in_string = true,
                '\'' if chars.get(i + 1) == Some(&'\\') => {
                    i += 2;
                    while i < chars.len() && chars[i] != '\'' {
                        i += 1;
                    }
                }
                '\'' if chars.get(i + 2) == Some(&'\'') => i += 2,
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    depth
}

/// Every `fn` with a body, as (line number, length in lines).
fn function_lengths(text: &str) -> Vec<(usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = Vec::new();
    for (start, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let is_fn = trimmed.starts_with("fn ") || trimmed.contains(" fn ") && !trimmed.starts_with("//");
        if !is_fn || trimmed.ends_with(';') {
            continue;
        }
        let mut depth = 0;
        let mut in_string = false;
        for (offset, body) in lines[start..].iter().enumerate() {
            let opened_before = depth > 0;
            depth += braces(body, &mut in_string);
            let opens_here = body.contains('{');
            if (opened_before || opens_here) && depth <= 0 {
                found.push((start + 1, offset + 1));
                break;
            }
            if !opened_before && !opens_here && body.trim_end().ends_with(';') {
                break;
            }
        }
    }
    found
}

#[test]
fn directories_hold_files_or_folders() {
    let mut bad = Vec::new();
    for krate in entries(&crates_dir()).1 {
        // A crate root holds Cargo.toml next to src/, as Cargo wants.
        for dir in entries(&krate).1.iter().filter(|d| !d.ends_with("target")) {
            check_shape(dir, dir.ends_with("src"), &mut bad);
        }
    }
    assert!(bad.is_empty(), "directories that mix files and folders:\n{}", bad.join("\n"));
}

fn check_shape(dir: &Path, is_src: bool, bad: &mut Vec<String>) {
    let (files, dirs) = entries(dir);
    let entry_only = files.iter().all(|f| {
        let name = f.file_name().unwrap();
        name == "main.rs" || name == "lib.rs"
    });
    let mixed = !files.is_empty() && !dirs.is_empty();
    if mixed && !(is_src && entry_only) {
        bad.push(dir.display().to_string());
    }
    for d in dirs {
        check_shape(&d, false, bad);
    }
}

#[test]
fn files_and_functions_stay_small() {
    let mut paths = Vec::new();
    for krate in entries(&crates_dir()).1 {
        for sub in ["src", "tests", "examples", "build.rs"] {
            let path = krate.join(sub);
            if path.is_dir() {
                rust_files(&path, &mut paths);
            } else if path.is_file() {
                paths.push(path);
            }
        }
    }
    let mut bad = Vec::new();
    for path in paths {
        let text = fs::read_to_string(&path).unwrap();
        let in_tests = path.components().any(|c| c.as_os_str() == "tests");
        let limit = if in_tests { TEST_FILE_LIMIT } else { SRC_FILE_LIMIT };
        let lines = code_lines(&text);
        if lines > limit {
            bad.push(format!("{}: {lines} code lines (limit {limit})", path.display()));
        }
        for (line, length) in function_lengths(&text) {
            if length > FUNCTION_LIMIT {
                bad.push(format!("{}:{line}: function of {length} lines", path.display()));
            }
        }
    }
    assert!(bad.is_empty(), "over the limits:\n{}", bad.join("\n"));
}

#[test]
fn function_length_counts_the_body() {
    let text = "fn a() {\n    x();\n}\nfn b();\n";
    assert_eq!(function_lengths(text), vec![(1, 3)]);
    assert_eq!(code_lines("// c\n\nx\n"), 1);
}
