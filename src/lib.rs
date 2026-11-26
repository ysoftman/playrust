pub mod highlight;
pub mod vim;

use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;

pub const LESSONS_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/lessons");
pub const BLANK: &str = "___";

const TIME_LIMIT: Duration = Duration::from_secs(5);
const MAX_LINES: usize = 200;
const MAX_BYTES: usize = 16 * 1024;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lesson {
    /// 파일명 stem (예: `01-hello`)
    #[serde(skip)]
    pub id: String,
    pub title: String,
    pub summary: String,
    pub concept: String,
    pub example: String,
    pub quiz: Vec<Quiz>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quiz {
    pub prompt: String,
    pub code: String,
    pub answers: Vec<String>,
    pub hint: String,
    pub explanation: String,
}

fn lesson_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    files.sort();
    Ok(files)
}

fn parse_lesson(path: &Path) -> Result<Lesson, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lesson: Lesson =
        toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    lesson.id = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    Ok(lesson)
}

/// 파일명 순으로 정렬된 레슨 목록
pub fn load_lessons(dir: &Path) -> Result<Vec<Lesson>, String> {
    let files = lesson_files(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    files.iter().map(|path| parse_lesson(path)).collect()
}

/// 모든 공백 문자 제거
fn normalize(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// 공백을 무시하고 대소문자는 구분해 비교
pub fn is_correct(input: &str, answer: &str) -> bool {
    normalize(input) == normalize(answer)
}

/// `___` 를 순서대로 fill(1), fill(2), ... 로 치환
pub fn fill_blanks(code: &str, fill: impl Fn(usize) -> String) -> String {
    let mut out = String::new();
    for (i, part) in code.split(BLANK).enumerate() {
        if i > 0 {
            out.push_str(&fill(i));
        }
        out.push_str(part);
    }
    out
}

#[derive(Debug)]
pub struct RunOutput {
    pub success: bool,
    /// 컴파일 실패면 rustc 에러, 성공이면 프로그램 출력(stdout + stderr)
    pub output: String,
}

/// 로컬 rustc 로 컴파일해 실행한다. 실행은 TIME_LIMIT 안에 끝나야 한다.
pub fn run_code(code: &str) -> RunOutput {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "playrust-run-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = compile_and_run(&dir, code);
    fs::remove_dir_all(&dir).ok();
    let (success, output) = result.unwrap_or_else(|e| (false, e));
    RunOutput { success, output }
}

fn compile_and_run(dir: &Path, code: &str) -> Result<(bool, String), String> {
    fs::create_dir_all(dir).map_err(|e| format!("Failed to create temp directory: {e}"))?;
    fs::write(dir.join("main.rs"), code)
        .map_err(|e| format!("Failed to write source file: {e}"))?;

    let compiled = Command::new("rustc")
        .args([
            "--edition",
            "2024",
            "--color",
            "never",
            "-o",
            "main",
            "main.rs",
        ])
        .current_dir(dir)
        .output()
        .map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => {
                "rustc not found. Install the Rust toolchain: https://rustup.rs".to_string()
            }
            _ => format!("Failed to run rustc: {e}"),
        })?;
    if !compiled.status.success() {
        return Ok((false, truncate(&String::from_utf8_lossy(&compiled.stderr))));
    }

    // 파이프 교착을 피하려고 stdout/stderr 를 같은 파일로 보낸다
    let log = dir.join("output");
    let out = File::create(&log).map_err(|e| format!("Failed to create output file: {e}"))?;
    let err = out
        .try_clone()
        .map_err(|e| format!("Failed to create output file: {e}"))?;
    let mut child = Command::new(dir.join("main"))
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(|e| format!("Failed to run program: {e}"))?;

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if start.elapsed() < TIME_LIMIT => thread::sleep(Duration::from_millis(10)),
            _ => {
                child.kill().ok();
                child.wait().ok();
                break None;
            }
        }
    };

    let mut output = truncate(&read_head(&log));
    let note = match status {
        Some(status) if status.success() => return Ok((true, output)),
        Some(status) => format!("(exited abnormally: {status})"),
        None => format!("Timed out: stopped after {}s.", TIME_LIMIT.as_secs()),
    };
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    output.push_str(&note);
    Ok((false, output))
}

/// 파일 앞부분만 읽는다 (MAX_BYTES 초과 여부를 알 수 있게 1바이트 더)
fn read_head(path: &Path) -> String {
    let mut buf = Vec::new();
    if let Ok(file) = File::open(path) {
        file.take(MAX_BYTES as u64 + 1).read_to_end(&mut buf).ok();
    }
    String::from_utf8_lossy(&buf).into_owned()
}

/// MAX_LINES 줄 / MAX_BYTES 바이트를 넘으면 잘라내고 표시를 붙인다
fn truncate(text: &str) -> String {
    let mut end = text.len().min(MAX_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    if let Some((i, _)) = text[..end].match_indices('\n').nth(MAX_LINES - 1) {
        end = i + 1;
    }
    if end == text.len() {
        return text.to_string();
    }
    let head = &text[..end];
    let sep = if head.ends_with('\n') { "" } else { "\n" };
    format!("{head}{sep}... (output truncated)")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_removes_all_whitespace() {
        assert_eq!(normalize("  let   mut\tx \n"), "letmutx");
        assert_eq!(normalize("   "), "");
    }

    #[test]
    fn answer_check_ignores_whitespace_and_is_case_sensitive() {
        assert!(is_correct(" mut ", "mut"));
        assert!(is_correct("& mut", "&mut"));
        assert!(is_correct("& self", "&self"));
        assert!(is_correct("if  let", "if let"));
        assert!(is_correct("&mut   String", "&mut String"));
        assert!(is_correct("&mutString", "&mut String"));
        assert!(is_correct(". .=", "..="));
        assert!(!is_correct("Mut", "mut"));
        assert!(!is_correct("mu", "mut"));
        assert!(!is_correct("mut x", "mut"));
    }

    #[test]
    fn fill_blanks_replaces_in_order() {
        let code = "let ___ x: ___ = 1;";
        assert_eq!(
            fill_blanks(code, |i| format!("[{i}]")),
            "let [1] x: [2] = 1;"
        );
        assert_eq!(
            fill_blanks("for i in 1___5 {}", |i| format!("[{i}]")),
            "for i in 1[1]5 {}"
        );
        assert_eq!(fill_blanks("no blank", |_| unreachable!()), "no blank");
    }

    #[test]
    fn truncate_limits_lines_and_bytes() {
        assert_eq!(truncate("a\nb\n"), "a\nb\n");

        let many_lines = "x\n".repeat(MAX_LINES + 1);
        let cut = truncate(&many_lines);
        assert!(cut.starts_with(&"x\n".repeat(MAX_LINES)));
        assert!(cut.ends_with("... (output truncated)"));

        let long_line = "가".repeat(MAX_BYTES);
        let cut = truncate(&long_line);
        assert!(cut.len() < MAX_BYTES + 100);
        assert!(cut.ends_with("... (output truncated)"));
    }

    #[test]
    fn load_lessons_fills_id_in_order() {
        let lessons = load_lessons(Path::new(LESSONS_DIR)).unwrap();
        let ids: Vec<&str> = lessons.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids.first(), Some(&"01-hello"));
        assert!(ids.is_sorted());
    }

    #[test]
    fn lessons_are_valid() {
        let files = lesson_files(Path::new(LESSONS_DIR)).expect("lessons 디렉터리를 읽을 수 없음");
        assert!(!files.is_empty(), "lessons/*.toml 이 없음");

        let mut errors = Vec::new();
        for path in &files {
            let lesson = match parse_lesson(path) {
                Ok(lesson) => lesson,
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            };
            let id = &lesson.id;
            if !(2..=3).contains(&lesson.quiz.len()) {
                errors.push(format!(
                    "{id}: quiz 는 2~3개여야 함 (현재 {})",
                    lesson.quiz.len()
                ));
            }
            for (n, quiz) in lesson.quiz.iter().enumerate() {
                let blanks = quiz.code.matches(BLANK).count();
                if !(1..=3).contains(&blanks) {
                    errors.push(format!(
                        "{id} quiz {}: 빈칸은 1~3개여야 함 (현재 {blanks})",
                        n + 1
                    ));
                }
                if blanks != quiz.answers.len() {
                    errors.push(format!(
                        "{id} quiz {}: 빈칸 {blanks}개 != answers {}개",
                        n + 1,
                        quiz.answers.len()
                    ));
                }
            }
        }
        assert!(errors.is_empty(), "레슨 검증 실패:\n{}", errors.join("\n"));
    }

    #[test]
    fn run_code_prints_output() {
        let out = run_code("fn main() { println!(\"hello, 러스트\"); }");
        assert!(out.success, "{}", out.output);
        assert_eq!(out.output, "hello, 러스트\n");
    }

    #[test]
    fn run_code_reports_compile_error() {
        let out = run_code("fn main() { let x: i32 = \"a\"; }");
        assert!(!out.success);
        assert!(out.output.contains("error"), "{}", out.output);
    }

    #[test]
    fn run_code_reports_panic() {
        let out = run_code("fn main() { panic!(\"boom\"); }");
        assert!(!out.success);
        assert!(out.output.contains("boom"), "{}", out.output);
        assert!(out.output.contains("exited abnormally"), "{}", out.output);
    }

    #[test]
    #[ignore = "시간 제한(5초)까지 기다림 (cargo test -- --ignored)"]
    fn run_code_times_out() {
        let out = run_code("fn main() { loop {} }");
        assert!(!out.success);
        assert!(out.output.contains("Timed out"), "{}", out.output);
    }

    /// example 과 정답을 채운 quiz 를 실행해 실패한 것만 모은다
    fn run_lesson(lesson: &Lesson) -> Vec<String> {
        let mut cases = vec![("example".to_string(), lesson.example.clone())];
        for (n, quiz) in lesson.quiz.iter().enumerate() {
            let filled = fill_blanks(&quiz.code, |i| {
                quiz.answers.get(i - 1).cloned().unwrap_or_default()
            });
            cases.push((format!("quiz {}", n + 1), filled));
        }
        cases
            .into_iter()
            .filter_map(|(label, source)| {
                let out = run_code(&source);
                (!out.success).then(|| format!("=== {} {label} ===\n{}", lesson.id, out.output))
            })
            .collect()
    }

    #[test]
    #[ignore = "모든 example 과 정답을 채운 quiz 를 rustc 로 컴파일·실행 (cargo test -- --ignored)"]
    fn lessons_compile_and_run() {
        let lessons = load_lessons(Path::new(LESSONS_DIR)).unwrap();
        let failures: Vec<String> = thread::scope(|s| {
            let handles: Vec<_> = lessons
                .iter()
                .map(|lesson| s.spawn(move || run_lesson(lesson)))
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap())
                .collect()
        });

        assert!(
            failures.is_empty(),
            "실패 {}건:\n\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}
