//! 앱 동작·화면 테스트. UI 동작과 색 경로는 이 파일의 고정 레슨(`fixture_app`)으로 검사하고,
//! 실제 `lessons/` 는 내용과 무관한 스모크 테스트(모든 화면이 패닉 없이 화면 안에 그려지는지)에만 쓴다

use std::path::Path;

use playrust::vim::{Editor, Mode};
use playrust::{BLANK, LESSONS_DIR, RunOutput, load_lessons};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::layout::Position;
use ratatui::style::{Color, Modifier};
use ratatui::text::Span;

use super::style::*;
use super::text::*;
use super::ui::*;
use super::*;

/// UI 테스트용 레슨 3개. 0: 출력 예제, 1: 변수 퀴즈(빈칸 1·2·2개), 2: `?`·`..=` 정답과 긴 예제·요약
const FIXTURE: [&str; 3] = [
    r#"
title = "첫 출력"
summary = "fn main, println!, 주석"
concept = '''
Rust 프로그램은 `fn main()` 함수에서 시작합니다. 한글 설명이 여러 줄로 이어집니다 가나다라마바사아자차카타파하 가나다라마바사아자차카타파하.

두 번째 문단도 한글로 길게 씁니다 가나다라마바사아자차카타파하 가나다라마바사아자차카타파하 가나다라마바사아자차카타파하.

세 번째 문단 가나다라마바사아자차카타파하 가나다라마바사아자차카타파하 가나다라마바사아자차카타파하 가나다라마바사.
'''
example = '''
// 프로그램은 main 에서 시작 (한글 주석은 좁은 화면에서 접힌다)
fn main() {
    println!("Hello, world!");
    println!("{} + {} = {}", 1, 2, 1 + 2);
}
'''

[[quiz]]
prompt = "빈칸에 출력 매크로 이름을 쓰세요"
code = '''
fn main() {
    ___!("hi");
}
'''
answers = ["println"]
hint = "`println!` 매크로"
explanation = "줄바꿈까지 출력합니다."

[[quiz]]
prompt = "함수를 정의하는 키워드"
code = '''
___ main() {}
'''
answers = ["fn"]
hint = "함수"
explanation = "`fn` 으로 함수를 정의합니다."
"#,
    r#"
title = "변수 연습"
summary = "let, mut, const, shadowing"
concept = '''
값을 바꾸려면 `let mut` 으로 선언하고 `x += 1` 처럼 바꿉니다.
'''
example = '''
const MAX_POINTS: u32 = 100;

fn main() {
    let mut count = 0;
    count += 1;
    println!("{count} / {MAX_POINTS}");
}
'''

[[quiz]]
prompt = "`score` 를 바꿀 수 있게 선언하세요"
code = '''
fn main() {
    let ___ score = 10;
    score += 5;
    println!("{score}");
}
'''
answers = ["mut"]
hint = "가변(mutable)의 줄임말"
explanation = "`let mut` 으로 선언하면 값을 바꿀 수 있습니다."

[[quiz]]
prompt = "같은 이름으로 다시 선언하세요"
code = '''
fn main() {
    let x = 5;
    ___ x = x + 1;
    ___ x = x * 2;
    println!("{x}");
}
'''
answers = ["let", "let"]
hint = "shadowing"
explanation = "`let` 을 다시 쓰면 새 변수가 생깁니다."

[[quiz]]
prompt = "상수와 가변 변수"
code = '''
___ LIMIT: u32 = 3;
fn main() {
    let ___ n = 0;
    n += LIMIT;
}
'''
answers = ["const", "mut"]
hint = "상수"
explanation = "상수는 `const` 로 선언합니다."
"#,
    r#"
title = "결과 처리"
summary = "Result 와 ? 연산자, parse 로 문자열을 숫자로 바꾸기, match 로 Ok 와 Err 를 나누는 방법까지 담은 아주 긴 요약"
concept = '''
실패할 수 있는 작업은 `Result<T, E>` 를 돌려줍니다.
'''
example = '''
use std::num::ParseIntError;

fn double(text: &str) -> Result<i32, ParseIntError> {
    let n = text.parse::<i32>()?;
    Ok(n * 2)
}

fn main() {
    match double("21") {
        Ok(v) => println!("{v}"),
        Err(e) => println!("error: {e}"),
    }
    for i in 1..=3 {
        println!("{i}");
    }
}
'''

[[quiz]]
prompt = "`?` 로 오류를 넘기세요"
code = '''
fn add(a: &str, b: &str) -> Result<i32, std::num::ParseIntError> {
    let x = a.parse::<i32>()___;
    let y = b.parse::<i32>()___;
    ___(x + y)
}
'''
answers = ["?", "?", "Ok"]
hint = "물음표 연산자"
explanation = "`?` 는 Err 면 바로 돌려줍니다."

[[quiz]]
prompt = "1 부터 5 까지 (5 포함)"
code = '''
fn main() {
    for i in 1___5 {
        println!("{i}");
    }
}
'''
answers = ["..="]
hint = "끝을 포함하는 범위"
explanation = "`..=` 는 끝을 포함합니다."
"#,
];

/// 스모크 테스트에 쓰는 화면 크기 (아주 작은 1x1 포함)
const SIZES: [(u16, u16); 5] = [(80, 24), (120, 40), (40, 12), (12, 5), (1, 1)];

fn fixture_app() -> App {
    let lessons = FIXTURE
        .iter()
        .map(|text| toml::from_str(text).expect("fixture 레슨"))
        .collect();
    App::new(lessons)
}

/// 실제 lessons/ 의 레슨 (내용과 무관한 스모크 테스트에만)
fn real_app() -> App {
    App::new(load_lessons(Path::new(LESSONS_DIR)).expect("레슨 로딩 실패"))
}

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
}

fn ctrl(app: &mut App, c: char) {
    app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL));
}

fn question(app: &mut App) {
    app.on_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT));
}

/// 목차 팝업을 열어 고른 줄(처음엔 현재 레슨)에서 Enter
fn toc_enter(app: &mut App) {
    ctrl(app, 'l');
    press(app, KeyCode::Enter);
}

/// leader(`,`) 다음에 c
fn leader(app: &mut App, c: char) {
    press(app, KeyCode::Char(','));
    press(app, KeyCode::Char(c));
}

fn type_str(app: &mut App, s: &str) {
    for c in s.chars() {
        press(app, KeyCode::Char(c));
    }
}

/// 레슨 보기를 offset 줄부터 보이게 (보기 커서는 화면 첫 줄, 화면은 커서를 따라가므로)
fn scroll_to(app: &App, offset: u16) {
    app.scroll.set(offset);
    app.cursor_row.set(offset);
}

/// 레슨 보기에서 ^O 로 퀴즈를 처음부터 (풀던 퀴즈가 있어도)
fn quiz_from_start(app: &mut App) {
    app.paused = None;
    ctrl(app, 'o');
}

/// lesson 번 레슨을 열고 첫 퀴즈로 (Insert 로 시작)
fn open_quiz(app: &mut App, lesson: usize) {
    app.open_lesson(lesson);
    quiz_from_start(app);
}

/// leader 키: Insert 면 `,` 가 글자라 Esc 로 나온 뒤 `,` c
fn quiz_leader(app: &mut App, c: char) {
    if app.editing_mode() == Some(Mode::Insert) {
        press(app, KeyCode::Esc);
    }
    leader(app, c);
}

/// 실행: Insert 면 Esc 로 나온 뒤 `,r`
fn run_key(app: &mut App) {
    quiz_leader(app, 'r');
}

fn show_answer(app: &mut App) {
    quiz_leader(app, 'a');
}

fn show_hint(app: &mut App) {
    quiz_leader(app, 'h');
}

/// 남은 퀴즈를 정답 보기로 넘겨 결과 화면까지
fn skip_quizzes(app: &mut App) {
    while app.screen == Screen::Quiz {
        show_answer(app);
        press(app, KeyCode::Enter);
    }
}

/// 현재 퀴즈의 정답 (레슨 내용을 테스트에 적지 않게)
fn answers(app: &App) -> Vec<String> {
    app.current_quiz().answers.clone()
}

/// 현재 퀴즈 코드에서 빈칸 줄을 fill 로 채워 앞뒤 공백을 뺀 것 (화면에서 찾을 때)
fn filled_line(app: &App, nth: usize, fill: &str) -> String {
    let line = app
        .current_quiz()
        .code
        .lines()
        .filter(|l| l.contains(BLANK))
        .nth(nth)
        .expect("빈칸 줄");
    line.trim().replace(BLANK, fill)
}

fn texts(app: &App) -> Vec<String> {
    app.quiz.inputs.iter().map(Editor::text).collect()
}

fn draw(app: &App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui(frame, app)).unwrap();
    terminal
}

fn buffer(app: &App, width: u16, height: u16) -> Buffer {
    draw(app, width, height).backend().buffer().clone()
}

/// 줄마다 (x, 글자) — 2칸 문자 뒤의 빈 셀은 건너뜀
fn cells(buf: &Buffer) -> Vec<Vec<(u16, String)>> {
    (0..buf.area.height)
        .map(|y| {
            let mut row = Vec::new();
            let mut skip = 0;
            for x in 0..buf.area.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let symbol = buf[(x, y)].symbol();
                skip = Span::raw(symbol).width().saturating_sub(1);
                row.push((x, symbol.to_string()));
            }
            row
        })
        .collect()
}

/// 화면을 그려 텍스트로 (어느 줄도 화면 폭을 넘지 않는지 함께 검사)
fn render(app: &App, width: u16, height: u16) -> String {
    let buf = buffer(app, width, height);
    for y in 0..buf.area.height {
        let last = buf[(buf.area.width - 1, y)].symbol();
        assert!(
            Span::raw(last).width() <= 1,
            "{width}x{height} {y}행이 화면 폭을 넘침"
        );
    }
    cells(&buf)
        .into_iter()
        .map(|row| row.into_iter().map(|(_, s)| s).collect::<String>() + "\n")
        .collect()
}

/// 하단 줄 (오른쪽 공백 제외)
fn foot(app: &App, width: u16) -> String {
    let screen = render(app, width, 24);
    screen.lines().last().unwrap().trim_end().to_string()
}

/// pat 이 처음 나오는 셀 좌표
fn find(buf: &Buffer, pat: &str) -> (u16, u16) {
    (0..buf.area.height)
        .find_map(|y| find_in_row(buf, y, pat).map(|x| (x, y)))
        .unwrap_or_else(|| panic!("화면에 {pat:?} 없음"))
}

/// y 행에서 pat 이 처음 나오는 셀의 x
fn find_in_row(buf: &Buffer, y: u16, pat: &str) -> Option<u16> {
    let row = &cells(buf)[usize::from(y)];
    let text: String = row.iter().map(|(_, s)| s.as_str()).collect();
    text.find(pat).map(|i| row[text[..i].chars().count()].0)
}

/// 화면에서 anchor 를 찾고, 그 안에서 token 이 처음 시작하는 셀 좌표
fn pos(buf: &Buffer, anchor: &str, token: &str) -> (u16, u16) {
    pos_in_row(buf, find(buf, anchor).1, anchor, token)
}

/// y 행의 anchor 안에서 token 이 처음 시작하는 셀 좌표
fn pos_in_row(buf: &Buffer, y: u16, anchor: &str, token: &str) -> (u16, u16) {
    let x = find_in_row(buf, y, anchor).unwrap_or_else(|| panic!("{y}행에 {anchor:?} 없음"));
    let i = anchor
        .find(token)
        .unwrap_or_else(|| panic!("{anchor:?} 에 {token:?} 없음"));
    (x + to_u16(Span::raw(&anchor[..i]).width()), y)
}

/// anchor 안 token 의 글자색
fn fg(buf: &Buffer, anchor: &str, token: &str) -> Color {
    buf[pos(buf, anchor, token)].fg
}

fn bold(buf: &Buffer, at: (u16, u16)) -> bool {
    buf[at].modifier.contains(Modifier::BOLD)
}

fn fake_output(success: bool) -> Output {
    let output = (1..=40)
        .map(|i| format!("{i}번째 줄 error[E0384]: cannot assign twice to immutable variable `score` 아주 긴 줄"))
        .collect::<Vec<_>>()
        .join("\n");
    Output::Done(RunOutput { success, output })
}

#[test]
fn renders_every_screen_without_panic() {
    // 스모크: 실제 레슨 전부, 모든 크기
    for (w, h) in SIZES {
        let mut app = real_app();
        app.start(None);
        render(&app, w, h);
        for i in 0..app.lessons.len() {
            app.open_lesson(i);
            ctrl(&mut app, 'l');
            render(&app, w, h);
            press(&mut app, KeyCode::Esc);
            render(&app, w, h);
            for _ in 0..30 {
                press(&mut app, KeyCode::PageDown);
            }
            render(&app, w, h);
            app.output = Some(Output::Running(String::new()));
            render(&app, w, h);
            app.output = Some(fake_output(true));
            render(&app, w, h);

            quiz_from_start(&mut app);
            for _ in 0..app.current().quiz.len() {
                assert_eq!(app.screen, Screen::Quiz);
                render(&app, w, h);
                type_str(&mut app, "오답 wrong");
                press(&mut app, KeyCode::Tab);
                press(&mut app, KeyCode::Enter);
                show_hint(&mut app);
                render(&app, w, h);
                press(&mut app, KeyCode::Esc);
                press(&mut app, KeyCode::Char('b'));
                render(&app, w, h);
                app.output = Some(fake_output(false));
                press(&mut app, KeyCode::Down);
                press(&mut app, KeyCode::PageDown);
                render(&app, w, h);
                show_answer(&mut app);
                render(&app, w, h);
                press(&mut app, KeyCode::Enter);
            }
            assert_eq!(app.screen, Screen::Result);
            render(&app, w, h);
        }
    }
}

#[test]
fn inline_edit_renders_every_lesson() {
    // 스모크: 실제 레슨 전부를 레슨 화면 안에서 편집 (아주 좁은 화면 포함), 커서는 늘 본문 안
    for (w, h) in SIZES.into_iter().chain([(5, 40), (4, 40), (3, 40)]) {
        let mut app = real_app();
        for i in 0..app.lessons.len() {
            app.open_lesson(i);
            render(&app, w, h);
            leader(&mut app, 'e');
            assert_eq!((app.screen, app.editing), (Screen::Lesson, true));
            render_editing(&app, w, h);
            // 맨 끝 줄 끝으로 가서 긴 한글 줄을 넣어 접힌 줄과 페이지 스크롤
            type_str(&mut app, "G$o");
            type_str(
                &mut app,
                "// 한글로 아주 길게 쓴 주석 줄입니다 가나다라마바사 아자차카타파하",
            );
            render_editing(&app, w, h);
            press(&mut app, KeyCode::Esc);
            type_str(&mut app, "bbb");
            render_editing(&app, w, h);
            type_str(&mut app, "gg$");
            render_editing(&app, w, h);
            app.output = Some(fake_output(false));
            for _ in 0..30 {
                press(&mut app, KeyCode::PageDown);
            }
            render_editing(&app, w, h);
            toc_enter(&mut app);
            assert_eq!((app.screen, app.editing), (Screen::Lesson, false));
            assert!(app.modified());
            render(&app, w, h);
        }
    }
}

#[test]
fn popups_render_on_any_size() {
    // 스모크: 실제 마지막 레슨 위의 모든 팝업
    let mut app = real_app();
    let last = app.lessons.len() - 1;
    for screen in [Screen::Lesson, Screen::Quiz, Screen::Result] {
        app.open_lesson(last);
        match screen {
            Screen::Quiz => app.start_quiz(0),
            other => app.goto(other),
        }
        for popup in [Popup::Keys, Popup::Quit, Popup::Lessons] {
            app.open(popup);
            for (w, h) in SIZES {
                render(&app, w, h);
            }
        }
        app.popup = None;
    }
}

#[test]
fn lesson_view_fits_narrow_screens() {
    // 스모크: 아주 좁은 화면에서도 실제 레슨 화면이 화면 폭을 넘지 않는다 (render 가 검사)
    let mut app = real_app();
    for i in 0..app.lessons.len() {
        app.open_lesson(i);
        for w in [5, 4, 3] {
            scroll_to(&app, 0);
            render(&app, w, 40);
            scroll_to(&app, u16::MAX);
            render(&app, w, 40);
        }
    }
}

#[test]
fn screens_show_expected_text() {
    let mut app = fixture_app();
    let n = app.lessons.len();
    app.start(None);
    let toc = render(&app, 80, 24);
    assert!(toc.contains(&format!(" Lessons 0/{n} ")), "{toc}");
    assert!(toc.contains(&format!("{n}. ")), "{toc}");
    app.popup = None;

    app.open_lesson(1);
    let lesson = render(&app, 80, 24);
    assert!(
        lesson.contains(&format!("2. {}", app.current().title)),
        "{lesson}"
    );

    quiz_from_start(&mut app);
    let quiz = render(&app, 80, 24);
    let total = app.current().quiz.len();
    assert!(quiz.contains(&format!("Quiz 1/{total}")), "{quiz}");
    assert!(quiz.contains(&filled_line(&app, 0, BLANK)), "{quiz}");
    let answer = answers(&app)[0].clone();
    type_str(&mut app, &answer);
    let quiz = render(&app, 80, 24);
    assert!(quiz.contains(&filled_line(&app, 0, &answer)), "{quiz}");
}

#[test]
fn help_lines_fit_in_80_columns() {
    // 넓은 화면에 그려 잘리지 않은 하단 줄의 폭을 잰다
    let bottom = |app: &App| {
        let line = foot(app, 200);
        assert!(Span::raw(line.as_str()).width() <= 80, "{line}");
        line
    };
    // 어느 화면에서나 목차(^L lessons)가 보인다
    let last = |app: &App| {
        let line = bottom(app);
        assert!(line.contains("^L lessons"), "{line}");
        line
    };
    let mut app = fixture_app();
    app.open_lesson(1);
    assert!(
        last(&app).starts_with("j/k move · ,r run"),
        "{}",
        last(&app)
    );
    assert!(last(&app).ends_with("? help · q quit"), "{}", last(&app));
    // 예제 편집은 두 모드 모두 ^O done editing, Normal 은 입력 전환 키도 안내 (팁 줄이 없으므로)
    leader(&mut app, 'e');
    assert!(
        last(&app).starts_with(" NORMAL  i/a/o insert"),
        "{}",
        last(&app)
    );
    assert!(last(&app).contains(",r run"), "{}", last(&app));
    assert!(last(&app).contains("^O done editing"), "{}", last(&app));
    assert!(last(&app).ends_with("? help"), "{}", last(&app));
    type_str(&mut app, "ox");
    assert!(last(&app).starts_with(" INSERT "), "{}", last(&app));
    assert!(last(&app).contains(",r run"), "{}", last(&app));
    assert!(last(&app).contains("^O done editing"), "{}", last(&app));
    assert!(last(&app).ends_with("^L lessons"), "{}", last(&app));
    press(&mut app, KeyCode::Esc);
    ctrl(&mut app, 'o');
    assert!(last(&app).starts_with(",r run"), "{}", last(&app));
    assert!(last(&app).contains("R reset"), "{}", last(&app));
    assert!(last(&app).ends_with("? help · q quit"), "{}", last(&app));

    // 퀴즈는 세 상태 모두 ^O lesson
    quiz_from_start(&mut app);
    assert!(last(&app).starts_with(" INSERT "), "{}", last(&app));
    assert!(last(&app).contains("^O lesson"), "{}", last(&app));
    press(&mut app, KeyCode::Esc);
    assert!(last(&app).starts_with(" NORMAL "), "{}", last(&app));
    assert!(last(&app).contains("^O lesson"), "{}", last(&app));
    assert!(last(&app).ends_with("? help"), "{}", last(&app));
    show_answer(&mut app);
    assert!(last(&app).starts_with("Enter next"), "{}", last(&app));
    assert!(last(&app).contains("^O lesson"), "{}", last(&app));
    assert!(last(&app).ends_with("? help"), "{}", last(&app));
    skip_quizzes(&mut app);
    assert_eq!(
        last(&app),
        "Enter next lesson · ^O lesson · ^L lessons · ? help · q quit"
    );
    app.lesson = app.lessons.len() - 1;
    assert_eq!(
        last(&app),
        "Enter lessons · ^O lesson · ^L lessons · ? help · q quit"
    );

    // 결과 화면에는 leader 로 할 일이 없어 , 를 눌러도 하단 줄이 그대로
    press(&mut app, KeyCode::Char(','));
    assert!(!app.leader);
    assert!(last(&app).starts_with("Enter lessons"), "{}", last(&app));

    // 팁과 팝업 줄도 80칸 화면에 들어간다 (팝업은 테두리·여백 4칸 포함)
    for tip in [QUIZ_TIP, BLANKS_TIP] {
        assert!(Span::raw(tip).width() <= 80, "{tip}");
    }
    for line in keys_lines() {
        assert!(line.width() + 4 <= 80, "{line}");
    }
    // 없어진 화면 전환 키는 도움말에 남지 않는다
    for gone in [",1", ",2", ",3", "1 list", "2 learn", "3 quiz", "Esc list"] {
        assert!(!KEYS.contains(gone), "{gone}");
    }
}

/// `?` 로 팝업이 열리고, 열려 있는 동안 다른 키는 무시되며, 닫는 키는 화면에 넘어가지 않는다
fn check_keys_popup(app: &mut App) {
    let state = |app: &App| {
        (
            app.screen,
            app.lesson,
            app.quiz.index,
            app.quiz.status,
            app.quiz.hint,
            texts(app),
            app.example_code(),
            app.editing_mode(),
        )
    };
    let before = state(app);
    question(app);
    assert_eq!(app.popup, Some(Popup::Keys), "{:?}", app.screen);
    assert!(!draw(app, 80, 24).backend().cursor_visible());
    let popup = render(app, 80, 24);
    assert!(
        popup.contains(" Keys ") && popup.contains("vim keys"),
        "{popup}"
    );

    for code in [
        KeyCode::Char('x'),
        KeyCode::Char('i'),
        KeyCode::Char('r'),
        KeyCode::Char('e'),
        KeyCode::Tab,
        KeyCode::Backspace,
        KeyCode::PageDown,
    ] {
        press(app, code);
    }
    for c in ['t', 'r', 'e'] {
        ctrl(app, c);
    }
    assert_eq!(app.popup, Some(Popup::Keys));
    assert!(app.pending_run().is_none() && !app.quit);
    assert_eq!(state(app), before);

    question(app);
    assert_eq!(app.popup, None);
    for code in [KeyCode::Esc, KeyCode::Char('q'), KeyCode::Enter] {
        question(app);
        assert_eq!(app.popup, Some(Popup::Keys));
        press(app, code);
        assert!(app.popup.is_none() && !app.quit, "{code:?}");
        assert_eq!(state(app), before, "{code:?}");
    }
    assert!(!render(app, 80, 24).contains(" Keys "));
}

#[test]
fn question_mark_opens_keys_popup() {
    let mut app = fixture_app();
    check_keys_popup(&mut app);
    app.open_lesson(2);
    check_keys_popup(&mut app);

    // 예제 편집: Normal 은 팝업, Insert 는 입력
    leader(&mut app, 'e');
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    check_keys_popup(&mut app);
    press(&mut app, KeyCode::Char('o'));
    question(&mut app);
    assert_eq!(app.popup, None);
    assert_eq!(app.draft().unwrap().lines()[1].trim(), "?");
    press(&mut app, KeyCode::Esc);
    toc_enter(&mut app);
    leader(&mut app, 'R');

    // 퀴즈: Insert 는 입력 (정답에 ? 가 들어가는 퀴즈도 있다), Normal 과 잠긴 뒤는 팝업
    quiz_from_start(&mut app);
    question(&mut app);
    assert_eq!(app.popup, None);
    assert_eq!(texts(&app)[0], "?");
    press(&mut app, KeyCode::Esc);
    check_keys_popup(&mut app);
    show_answer(&mut app);
    check_keys_popup(&mut app);
    skip_quizzes(&mut app);
    assert_eq!(app.screen, Screen::Result);
    check_keys_popup(&mut app);

    // 도움말 팝업 위의 Ctrl+C 는 종료 확인으로 바뀐다
    question(&mut app);
    ctrl(&mut app, 'c');
    assert_eq!((app.popup, app.quit), (Some(Popup::Quit), false));
    ctrl(&mut app, 'c');
    assert!(app.quit);
}

#[test]
fn r_question_replaces_char_in_normal() {
    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    type_str(&mut app, "mut");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    question(&mut app);
    assert_eq!(app.popup, None);
    assert_eq!(texts(&app), vec!["mu?"]);
    question(&mut app);
    assert_eq!(app.popup, Some(Popup::Keys));
    press(&mut app, KeyCode::Esc);

    // 예제 편집도 같다
    toc_enter(&mut app);
    leader(&mut app, 'e');
    press(&mut app, KeyCode::Char('0'));
    press(&mut app, KeyCode::Char('r'));
    question(&mut app);
    assert_eq!(app.popup, None);
    assert!(app.draft().unwrap().lines()[0].starts_with('?'));
    assert!(app.modified());
}

#[test]
fn quit_asks_to_confirm() {
    // 레슨 보기 q → 확인 팝업 (그 외 키 무시) → n 취소, 다시 q → y 종료
    let mut app = fixture_app();
    press(&mut app, KeyCode::Char('q'));
    assert_eq!((app.popup, app.quit), (Some(Popup::Quit), false));
    let screen = render(&app, 80, 24);
    assert!(
        screen.contains(" Quit ")
            && screen.contains("Quit playrust?")
            && screen.contains("y/Enter quit · n/Esc cancel"),
        "{screen}"
    );
    for code in [
        KeyCode::Char('j'),
        KeyCode::Char('x'),
        KeyCode::Down,
        KeyCode::Tab,
    ] {
        press(&mut app, code);
    }
    ctrl(&mut app, 'l');
    assert_eq!(
        (app.popup, app.screen, app.lesson, app.quit),
        (Some(Popup::Quit), Screen::Lesson, 0, false)
    );
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(
        (app.popup, app.screen, app.quit),
        (None, Screen::Lesson, false)
    );
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Char('y'));
    assert!(app.quit);

    // 레슨 보기 Esc 는 습관적으로 눌러도 아무 일 없고, 확인 팝업은 Y/Enter 로도 종료
    for confirm in [KeyCode::Char('Y'), KeyCode::Enter] {
        let mut app = fixture_app();
        press(&mut app, KeyCode::Esc);
        assert_eq!((app.popup, app.quit), (None, false));
        press(&mut app, KeyCode::Char('q'));
        press(&mut app, confirm);
        assert!(app.quit, "{confirm:?}");
    }

    // 레슨 보기 q: N/Esc/q 는 취소하고 화면 그대로
    let mut app = fixture_app();
    app.open_lesson(1);
    for cancel in [KeyCode::Char('N'), KeyCode::Esc, KeyCode::Char('q')] {
        press(&mut app, KeyCode::Char('q'));
        assert_eq!(app.popup, Some(Popup::Quit));
        press(&mut app, cancel);
        assert_eq!(
            (app.popup, app.screen, app.lesson, app.quit),
            (None, Screen::Lesson, 1, false),
            "{cancel:?}"
        );
    }
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Char('y'));
    assert!(app.quit);

    // 퀴즈 INSERT 의 q/y/n 은 입력, Ctrl+C → 확인 팝업 → Ctrl+C 즉시 종료
    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    type_str(&mut app, "qyn");
    assert_eq!((app.popup, app.quit), (None, false));
    assert_eq!(texts(&app), vec!["qyn"]);
    ctrl(&mut app, 'c');
    assert_eq!((app.popup, app.quit), (Some(Popup::Quit), false));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.popup, None);
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    assert_eq!(texts(&app), vec!["qyn"]);
    ctrl(&mut app, 'c');
    ctrl(&mut app, 'c');
    assert!(app.quit);
}

#[test]
fn lessons_popup_jumps_anywhere() {
    // 레슨 보기에서 열면 현재 레슨에서 시작, Ctrl+L 로 닫기
    let mut app = fixture_app();
    let n = app.lessons.len();
    app.open_lesson(n - 1);
    ctrl(&mut app, 'l');
    assert_eq!((app.popup, app.pick), (Some(Popup::Lessons), n - 1));
    let screen = render(&app, 80, 24);
    assert!(
        screen.contains(" Lessons ")
            && screen.contains(&format!(">  {n}. "))
            && screen.contains(" 1. "),
        "{screen}"
    );
    ctrl(&mut app, 'l');
    assert_eq!((app.popup, app.screen), (None, Screen::Lesson));

    // 퀴즈 INSERT: Ctrl+L 은 입력되지 않고, 처음 선택은 현재 레슨
    open_quiz(&mut app, 0);
    type_str(&mut app, "mu");
    ctrl(&mut app, 'l');
    assert_eq!((app.popup, app.pick), (Some(Popup::Lessons), 0));
    assert_eq!(texts(&app), vec!["mu"]);
    // 현재 레슨은 강조 (고른 줄이 아니어도)
    type_str(&mut app, "jjx?");
    assert_eq!((app.popup, app.pick), (Some(Popup::Lessons), 2));
    let buf = buffer(&app, 80, 24);
    let (_, top) = find(&buf, "┌ Lessons");
    let current = format!(" 1. {}", app.lessons[0].title);
    let title = pos_in_row(&buf, top + 1, &current, &app.lessons[0].title);
    assert_eq!((buf[title].fg, buf[title].bg), (BLUE, Color::Reset));
    assert!(bold(&buf, title));
    assert!(find_in_row(&buf, top + 3, ">  3. ").is_some());
    // Esc 로 닫으면 화면·입력값·모드 그대로
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.pick, 1);
    press(&mut app, KeyCode::Esc);
    assert_eq!((app.popup, app.screen, app.lesson), (None, Screen::Quiz, 0));
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    type_str(&mut app, "t");
    assert_eq!(texts(&app), vec!["mut"]);

    // j 후 Enter 로 다른 레슨 화면 (퀴즈를 떠난다)
    ctrl(&mut app, 'l');
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        (app.popup, app.screen, app.lesson),
        (None, Screen::Lesson, 1)
    );

    // 예제 편집 INSERT: 입력되지 않고, q 로 닫으면 이어서 편집
    leader(&mut app, 'e');
    press(&mut app, KeyCode::Char('o'));
    type_str(&mut app, "// 메모");
    ctrl(&mut app, 'l');
    press(&mut app, KeyCode::Char('q'));
    assert_eq!(
        (app.popup, app.screen, app.editing),
        (None, Screen::Lesson, true)
    );
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    assert_eq!(app.draft().unwrap().lines()[1].trim(), "// 메모");
    // 다른 레슨으로 가도 수정본은 Normal 로 남고, 수정 안 한 편집은 버린다
    ctrl(&mut app, 'l');
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        (app.screen, app.lesson, app.editing),
        (Screen::Lesson, 0, false)
    );
    let draft = app.drafts[1].as_ref().unwrap();
    assert_eq!(draft.mode(), Mode::Normal);
    assert!(draft.text().contains("// 메모"));
    leader(&mut app, 'e');
    ctrl(&mut app, 'l');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(app.drafts[0].is_none());
    app.open_lesson(1);
    assert!(app.modified());

    // 낮은 화면에서는 고른 레슨을 따라 스크롤
    ctrl(&mut app, 'l');
    for _ in 0..20 {
        press(&mut app, KeyCode::Char('j'));
    }
    assert_eq!(app.pick, n - 1);
    let low = render(&app, 40, 4);
    assert!(
        low.contains(&format!(">  {n}. ")) && !low.contains(" 1. "),
        "{low}"
    );
}

/// 고른 줄은 반전(흰 막대)이 아니라 x0..x1 전체가 초록(CursorLine) 바탕
/// (2칸 문자의 뒤 칸은 터미널에 따로 그려지지 않으므로 건너뜀)
fn assert_selected_row(buf: &Buffer, y: u16, x0: u16, x1: u16) {
    for (x, _) in &cells(buf)[usize::from(y)] {
        if !(x0..x1).contains(x) {
            continue;
        }
        let cell = &buf[(*x, y)];
        assert_eq!(cell.bg, CURSORLINE, "({x}, {y})");
        assert!(!cell.modifier.contains(Modifier::REVERSED), "({x}, {y})");
    }
}

#[test]
fn toc_selection_is_green_not_reversed() {
    let mut app = fixture_app();
    let n = app.lessons.len();
    app.done[2] = true;
    app.lesson = 2;
    // 요약은 잘릴 수 있어 첫 단어만 찾는다
    let entry = |app: &App, i: usize| {
        let lesson = &app.lessons[i];
        let head = lesson
            .summary
            .split_whitespace()
            .next()
            .unwrap()
            .to_string();
        (lesson.title.clone(), head)
    };
    let (title2, head2) = entry(&app, 2);
    let (title1, head1) = entry(&app, 1);

    // 처음 고른 줄은 현재 레슨: 기호 초록, 번호 보라, 현재 레슨 제목은 파랑 굵게, ✓ 초록,
    // 요약은 주석색. 제목에 완료 수, 고르지 않은 줄은 바탕 없음
    ctrl(&mut app, 'l');
    let buf = buffer(&app, 80, 24);
    let (left, top) = find(&buf, &format!("┌ Lessons 1/{n} "));
    let right = find_in_row(&buf, top, "┐").unwrap();
    let row = format!(">  3. {title2} ✓  {head2}");
    let (_, y) = find(&buf, &row);
    assert_selected_row(&buf, y, left + 2, right - 1);
    assert_eq!(fg(&buf, &row, ">"), GREEN);
    assert!(bold(&buf, pos(&buf, &row, ">")));
    assert_eq!(fg(&buf, &row, "3"), PURPLE);
    assert_eq!(fg(&buf, &row, &title2), BLUE);
    assert!(bold(&buf, pos(&buf, &row, &title2)));
    assert_eq!(fg(&buf, &row, "✓"), GREEN);
    assert_eq!(fg(&buf, &row, &head2), COMMENT);
    let other = format!(" 2. {title1}  {head1}");
    let number = pos(&buf, &other, "2");
    assert_eq!((buf[number].fg, buf[number].bg), (COMMENT, Color::Reset));
    assert_eq!(fg(&buf, &other, &head1), COMMENT);

    // 위로 옮기면 그 줄이 같은 선택 스타일. 테두리 회색, 제목 초록, 아래 안내는 주석색
    press(&mut app, KeyCode::Char('k'));
    let buf = buffer(&app, 80, 24);
    let (left, top) = find(&buf, "┌ Lessons");
    assert_eq!(buf[(left, top)].fg, GRAY);
    assert_eq!(fg(&buf, "┌ Lessons", "Lessons"), GREEN);
    assert!(bold(&buf, pos(&buf, "┌ Lessons", "Lessons")));
    assert_eq!(fg(&buf, "Enter open", "Enter"), COMMENT);
    let picked = format!(">  2. {title1}");
    let (_, y) = find(&buf, &picked);
    let right = find_in_row(&buf, top, "┐").unwrap();
    assert_selected_row(&buf, y, left + 2, right - 1);
    assert_eq!(buf[(left + 1, y)].bg, Color::Reset);
    assert_eq!(
        (fg(&buf, &picked, ">"), fg(&buf, &picked, "2")),
        (GREEN, PURPLE)
    );
    assert_eq!(fg(&buf, &picked, &title1), Color::Reset);
    // 현재 레슨(3)은 파랑 굵게, ✓ 초록
    let current = format!(" 3. {title2} ✓");
    assert_eq!(fg(&buf, &current, "3"), COMMENT);
    assert_eq!(fg(&buf, &current, &title2), BLUE);
    assert!(bold(&buf, pos(&buf, &current, &title2)));
    assert_eq!(fg(&buf, &current, "✓"), GREEN);
}

#[test]
fn keys_popup_renders_on_any_size() {
    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    press(&mut app, KeyCode::Esc);
    question(&mut app);
    for (w, h) in SIZES {
        app.keys_scroll.set(0);
        render(&app, w, h);
        for _ in 0..50 {
            press(&mut app, KeyCode::Down);
        }
        render(&app, w, h);
    }

    // 80x24 는 스크롤 없이 다 보이고 머리줄·하단 줄도 가리지 않는다
    app.keys_scroll.set(0);
    let full = render(&app, 80, 24);
    assert!(
        full.contains("Screens") && full.contains("Unsupported"),
        "{full}"
    );
    let header = format!("Quiz 1/{}", app.current().quiz.len());
    assert!(full.lines().next().unwrap().contains(&header), "{full}");
    assert!(full.lines().last().unwrap().contains(" NORMAL "), "{full}");

    // 한글 배경(레슨 1 설명) 위에서도 팝업 왼쪽 테두리 앞 칸에 2칸 문자가 걸치지 않는다
    app.popup = None;
    app.open_lesson(0);
    question(&mut app);
    let buf = buffer(&app, 80, 24);
    let (x, top) = find(&buf, "┌ Keys");
    let bottom = top + to_u16(keys_lines().len()) + 1;
    for y in top..=bottom {
        let border = match y {
            _ if y == top => "┌",
            _ if y == bottom => "└",
            _ => "│",
        };
        assert_eq!(buf[(x, y)].symbol(), border, "{y}행");
        assert!(Span::raw(buf[(x - 1, y)].symbol()).width() <= 1, "{y}행");
    }
    app.popup = None;
    open_quiz(&mut app, 1);
    press(&mut app, KeyCode::Esc);
    question(&mut app);

    // 40x12 는 넘쳐서 j/k 로 스크롤 (끝에서 멈춤)
    assert!(!render(&app, 40, 12).contains("redo"));
    for _ in 0..50 {
        press(&mut app, KeyCode::Char('j'));
    }
    let bottom = render(&app, 40, 12);
    assert!(bottom.contains("redo"), "{bottom}");
    let end = app.keys_scroll.get();
    assert!(end > 0);
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.keys_scroll.get(), end - 1);
}

#[test]
fn tips_show_in_empty_panels() {
    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    assert!(render(&app, 80, 24).contains(QUIZ_TIP));
    press(&mut app, KeyCode::Esc);
    assert!(render(&app, 80, 24).contains(QUIZ_TIP));
    show_hint(&mut app);
    let hint = render(&app, 80, 24);
    assert!(hint.contains("Hint:") && !hint.contains("Tip:"), "{hint}");
    show_hint(&mut app);
    app.output = Some(fake_output(true));
    assert!(!render(&app, 80, 24).contains("Tip:"));

    // 예제 편집은 보기와 레이아웃이 같도록 팁 줄 없이, 입력 전환 키는 하단 줄에 안내
    ctrl(&mut app, 'o');
    leader(&mut app, 'e');
    let screen = render(&app, 80, 24);
    assert!(
        !screen.contains("Tip:") && screen.contains("i/a/o insert"),
        "{screen}"
    );
    run_key(&mut app);
    assert!(!render(&app, 80, 24).contains("Tip:"));
    for (w, h) in SIZES {
        app.output = None;
        render(&app, w, h);
        app.open_lesson(1);
        app.start_quiz(0);
        render(&app, w, h);
        press(&mut app, KeyCode::Esc);
        ctrl(&mut app, 'o');
        leader(&mut app, 'e');
    }
}

#[test]
fn code_is_highlighted() {
    let mut app = fixture_app();
    app.open_lesson(0);
    let buf = buffer(&app, 80, 60);
    let line = "│ fn main() {";
    assert_eq!(fg(&buf, line, "│"), GRAY);
    assert_eq!(fg(&buf, line, "fn"), PURPLE);
    assert_eq!(fg(&buf, line, "main"), BLUE);
    // 정의 괄호·블록 괄호는 주황
    assert_eq!(fg(&buf, line, "("), ORANGE);
    assert_eq!(fg(&buf, line, "{"), ORANGE);
    // 매크로는 rust-analyzer 처럼 주황 (굵게 없음), 매크로 괄호는 청록
    let line = "println!(\"Hello";
    assert_eq!(fg(&buf, line, "println!"), ORANGE);
    assert!(!bold(&buf, pos(&buf, line, "println!")));
    assert_eq!(fg(&buf, line, "("), CYAN);
    assert_eq!(fg(&buf, line, "\""), GREEN);

    // 편집 중에도 하이라이트, 주석은 회색 글자 (이탤릭 없음). 머리줄 요약에도 fn main 이 있어 박스 안에서 찾는다
    leader(&mut app, 'e');
    let buf = buffer(&app, 80, 24);
    assert_eq!(fg(&buf, "│ fn main", "fn"), PURPLE);
    let comment = pos(&buf, "// 프로그램은", "//");
    assert_eq!(buf[comment].fg, COMMENT);
    assert!(buf[comment].modifier.is_empty());

    // 퀴즈 코드: 키워드는 색, 빈칸은 입력칸(밑줄)이고 커서가 그 자리에
    open_quiz(&mut app, 1);
    let mut terminal = draw(&app, 80, 24);
    let buf = terminal.backend().buffer().clone();
    // 변수는 빨강, 복합 대입은 rust-analyzer 처럼 청록 (굵게 없음)
    let line = "score += 5";
    assert_eq!(fg(&buf, line, "score"), RED);
    assert_eq!(fg(&buf, line, "+="), CYAN);
    assert!(buf[pos(&buf, line, "+=")].modifier.is_empty());
    assert_eq!(fg(&buf, line, "5"), ORANGE);
    assert_eq!(fg(&buf, "\"{score}\"", "\""), GREEN);
    let line = filled_line(&app, 0, BLANK);
    assert_eq!(fg(&buf, &line, "let"), PURPLE);
    let blank = pos(&buf, &line, BLANK);
    assert!(buf[blank].modifier.contains(Modifier::UNDERLINED));
    assert_eq!(
        terminal.get_cursor_position().unwrap(),
        Position::from(blank)
    );
}

#[test]
fn example_highlights_calls_placeholders_and_operators() {
    let mut app = fixture_app();
    app.open_lesson(0);
    let buf = buffer(&app, 80, 60);
    // 자리표시자는 rust-analyzer 처럼 문자열과 같은 초록, 문자열 밖 연산자는 기본 글자색 (굵게 없음)
    let line = "\"{} + {} = {}\", 1, 2, 1 + 2";
    for token in ["\"", "{", "}", "+"] {
        assert_eq!(fg(&buf, line, token), GREEN, "{token}");
    }
    assert_eq!(fg(&buf, line, "1 + 2"), ORANGE);
    assert_eq!(fg(&buf, line, "+ 2"), Color::Reset);
    assert!(!bold(&buf, pos(&buf, line, "+ 2")));

    app.open_lesson(1);
    let buf = buffer(&app, 80, 60);
    let line = "│ const MAX_POINTS: u32";
    assert_eq!(fg(&buf, line, "const"), PURPLE);
    assert_eq!(fg(&buf, line, "MAX_POINTS"), ORANGE);
    assert_eq!(fg(&buf, line, "u32"), YELLOW);

    app.open_lesson(2);
    let buf = buffer(&app, 80, 60);
    let line = "-> Result<i32";
    assert_eq!(fg(&buf, line, "->"), Color::Reset);
    assert_eq!(fg(&buf, line, "Result"), YELLOW);
    let line = "text.parse::<i32>()?";
    assert_eq!(fg(&buf, line, "parse"), BLUE);
    // 값 뒤의 ? 는 청록, 그 밖의 연산자(::)는 기본 글자색
    assert_eq!(fg(&buf, line, "::"), Color::Reset);
    assert_eq!(fg(&buf, line, "?"), CYAN);
    // 함수 호출 괄호는 보라 (onedarkpro 의 Rust 인자 괄호)
    let line = "match double(";
    assert_eq!(fg(&buf, line, "double"), BLUE);
    assert_eq!(fg(&buf, line, "("), PURPLE);
    let line = "\"{v}\"";
    assert_eq!(fg(&buf, line, "{v}"), GREEN);
    assert_eq!(fg(&buf, line, "\""), GREEN);
    // 예제 코드의 어떤 글자도 굵게·이탤릭이 아니다
    let (_, top) = find(&buf, "┌ Example");
    for y in top + 1..buf.area.height - 1 {
        for x in 0..buf.area.width {
            assert!(buf[(x, y)].modifier.is_empty(), "({x}, {y})");
        }
    }
}

#[test]
fn inline_code_in_text_is_highlighted_on_screens() {
    // 레슨 개념 설명과 퀴즈 해설 화면에서도 (rich 자체는 style.rs 테스트)
    let mut app = fixture_app();
    app.open_lesson(0);
    let buf = buffer(&app, 80, 24);
    let line = "fn main() 함수";
    assert_eq!(fg(&buf, line, "fn"), PURPLE);
    assert_eq!(fg(&buf, line, "main"), BLUE);
    assert_eq!(fg(&buf, line, "함수"), Color::Reset);

    app.open_lesson(1);
    let buf = buffer(&app, 80, 24);
    let line = "x += 1 처럼";
    assert_eq!(fg(&buf, line, "+="), CYAN);
    assert!(!bold(&buf, pos(&buf, line, "+=")));
    assert_eq!(fg(&buf, line, "1"), ORANGE);
    assert_eq!(fg(&buf, line, " 처럼"), Color::Reset);

    app.start_quiz(0);
    show_answer(&mut app);
    let buf = buffer(&app, 120, 30);
    let line = "let mut 으로";
    assert_eq!(fg(&buf, line, "let"), PURPLE);
    assert_eq!(fg(&buf, line, "mut"), PURPLE);
    assert_eq!(fg(&buf, line, "으로"), Color::Reset);
}

#[test]
fn blank_in_middle_of_token_has_no_padding() {
    let mut app = fixture_app();
    app.open_lesson(2);
    app.start_quiz(1);
    let answer = answers(&app)[0].clone();
    type_str(&mut app, &answer);
    let quiz = render(&app, 80, 24);
    let line = filled_line(&app, 0, &answer);
    assert!(quiz.contains(&line), "{line:?}\n{quiz}");
}

/// 화면을 그려 본문 줄만 (머리줄·하단 줄 제외)
fn body_rows(app: &App, w: u16, h: u16) -> Vec<String> {
    let screen = render(app, w, h);
    let rows: Vec<String> = screen.lines().map(str::to_string).collect();
    rows[1..rows.len() - 1].to_vec()
}

#[test]
fn example_box_survives_scroll_and_wrap() {
    let mut app = fixture_app();
    app.open_lesson(0);
    let example: String = app.current().example.split_whitespace().collect();
    for w in [80, 40, 12, 7] {
        // 전체가 보이는 높이로 그린 본문이 기준
        scroll_to(&app, 0);
        let full = body_rows(&app, w, 2000);
        let top = full.iter().position(|r| r.starts_with('┌')).unwrap();
        let len = full.iter().position(|r| r.starts_with('└')).unwrap() + 1;
        assert_eq!(full[top - 1].trim(), "", "{w}: 설명과 박스 사이 빈 줄");
        if w > 10 {
            assert!(full[top].starts_with("┌ Example ─"), "{w}: {}", full[top]);
        }
        // 박스는 화면 폭을 꽉 채우고(아래 실행 패널과 같은 폭),
        // 한글 주석이 줄바꿈돼도 테두리를 지우지 않고 글자도 빠지지 않는다
        let mut inner = String::new();
        for (i, row) in full[top..len].iter().enumerate() {
            let row = row.trim_end();
            let (first, last) = match i {
                0 => ('┌', '┐'),
                _ if top + i == len - 1 => ('└', '┘'),
                _ => ('│', '│'),
            };
            assert!(
                row.starts_with(first) && row.ends_with(last),
                "{w}: {row:?}"
            );
            assert_eq!(Span::raw(row).width(), usize::from(w), "{w}: {row:?}");
            if first == '│' {
                inner.push_str(row.trim_matches('│'));
            }
        }
        let inner: String = inner.split_whitespace().collect();
        assert_eq!(inner, example, "{w}");

        // 어느 스크롤 위치에서도 보이는 줄은 전체 본문의 그 구간과 같고(박스 일부만 보여도
        // 테두리가 온전), 끝까지 내리면 박스 아래 테두리가 맨 아래 줄
        for h in [24, 12, 6] {
            let view = usize::from(h - 2);
            let max = len.saturating_sub(view);
            scroll_to(&app, u16::MAX);
            let rows = body_rows(&app, w, h);
            assert_eq!(usize::from(app.scroll.get()), max, "{w}x{h}");
            if len > view {
                assert!(rows[view - 1].starts_with('└'), "{w}x{h}: {rows:?}");
            }
            for offset in 0..=max {
                scroll_to(&app, to_u16(offset));
                let rows = body_rows(&app, w, h);
                assert_eq!(rows[..], full[offset..offset + view], "{w}x{h} {offset}");
            }
        }
    }

    // 실행 출력 패널이 있어도 박스 오른쪽 테두리는 패널과 같은 마지막 칸
    app.output = Some(fake_output(true));
    scroll_to(&app, u16::MAX);
    let buf = buffer(&app, 40, 24);
    let (_, panel) = find(&buf, "┌ Output");
    for y in [panel - 1, panel, panel + 1] {
        let border = if y == panel { "┐" } else { "│" };
        let border = if y == panel - 1 { "┘" } else { border };
        assert_eq!(buf[(39, y)].symbol(), border, "{y}");
    }
    app.output = None;

    // 편집 중에도 같은 박스 (테두리는 모드 색: Insert 파랑, 라벨 초록), 긴 한글 줄도 접혀 오른쪽 테두리 유지
    leader(&mut app, 'e');
    type_str(&mut app, "A // 아주 길게 쓴 한글 주석 가나다라마바사");
    for w in [80, 40, 13] {
        let buf = buffer(&app, w, 12);
        let rows: Vec<u16> = (1..buf.area.height - 1)
            .filter(|&y| buf[(0, y)].symbol() == "│")
            .collect();
        assert!(!rows.is_empty(), "{w}");
        for y in rows {
            assert_eq!(buf[(0, y)].fg, BLUE, "{w} {y}");
            assert_eq!(buf[(w - 1, y)].symbol(), "│", "{w} {y}");
            assert!(Span::raw(buf[(w - 2, y)].symbol()).width() <= 1, "{w} {y}");
        }
        // 13칸에서는 라벨이 오른쪽 위치 표시에 밀려 " Exam" 까지 보인다
        let buf = buffer(&app, w, 200);
        let (x, y) = find(&buf, "┌ Exam");
        assert_eq!((buf[(x, y)].fg, buf[(x + 2, y)].fg), (BLUE, GREEN));
    }
}

#[test]
fn graded_blanks_have_background() {
    // 틀린 빈칸은 빨간 바탕, 맞은 빈칸은 초록 바탕이라 빨강 변수·초록 문자열 글자와 구분된다
    let mut app = fixture_app();
    app.open_lesson(1);
    app.start_quiz(1);
    let right = answers(&app)[0].clone();
    type_str(&mut app, &right);
    press(&mut app, KeyCode::Tab);
    type_str(&mut app, "x");
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.marks, vec![Some(true), Some(false)]);
    press(&mut app, KeyCode::Tab);
    let buf = buffer(&app, 80, 24);
    let wrong = &buf[pos(&buf, &filled_line(&app, 1, "x"), "x")];
    assert_eq!((wrong.fg, wrong.bg), (RED, WRONG_BG));
    assert!(
        wrong
            .modifier
            .contains(Modifier::BOLD | Modifier::UNDERLINED)
    );
    let right = &buf[pos(&buf, &filled_line(&app, 0, &right), &right)];
    // 현재 빈칸(반전)이 맞은 빈칸이면 초록 칸에 진한 초록 글자
    assert_eq!((right.fg, right.bg), (GREEN, CURSORLINE));
    assert!(right.modifier.contains(Modifier::REVERSED));
    let screen = render(&app, 80, 24);
    assert!(screen.contains("✗ Some blanks are wrong (red)"), "{screen}");
}

#[test]
fn quiz_flow_on_lesson_02() {
    // 시작 목차 팝업에서 레슨 2 를 골라 연다
    let mut app = fixture_app();
    app.start(None);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.screen, app.lesson), (Screen::Lesson, 1));
    quiz_from_start(&mut app);
    assert_eq!((app.screen, app.quiz.index), (Screen::Quiz, 0));

    // 오답 (정답의 마지막 글자만 틀림)
    let answer = answers(&app)[0].clone();
    let (head, last) = answer.split_at(answer.len() - 1);
    type_str(&mut app, &format!("{head}x"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.marks, vec![Some(false)]);
    assert_eq!((app.quiz.status, app.score), (Status::Answering, 0));

    // 고치면 표시가 지워지고, 정답이면 점수 +1
    press(&mut app, KeyCode::Backspace);
    type_str(&mut app, last);
    assert_eq!(app.quiz.marks, vec![None]);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.marks, vec![Some(true)]);
    assert_eq!((app.quiz.status, app.score), (Status::Solved, 1));
    type_str(&mut app, "x");
    assert_eq!(texts(&app), vec![answer]);

    // Tab / Shift+Tab 이동
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.quiz.index, app.quiz.blank), (1, 0));
    let answer = answers(&app);
    type_str(&mut app, &answer[0]);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.quiz.blank, 1);
    let (head, last) = answer[1].split_at(answer[1].len() - 1);
    type_str(&mut app, head);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.quiz.blank, 0);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.quiz.blank, 1);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.marks, vec![Some(true), Some(false)]);
    assert_eq!(app.quiz.blank, 1);
    type_str(&mut app, last);
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.quiz.status, app.score), (Status::Solved, 2));

    // Ctrl+E 정답 보기는 점수 미반영
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.index, 2);
    show_answer(&mut app);
    assert_eq!(app.quiz.status, Status::Revealed);
    assert_eq!(texts(&app), answers(&app));
    type_str(&mut app, "x");
    assert_eq!(texts(&app), answers(&app));
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.screen, app.score), (Screen::Result, 2));
    assert!(!app.done[1]);

    // 결과 → 다음 레슨
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.screen, app.lesson), (Screen::Lesson, 2));
}

#[test]
fn quiz_normal_mode_edits_current_blank() {
    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    let answer = answers(&app)[0].clone();
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.marks, vec![Some(false)]);
    type_str(&mut app, &format!("{answer}x"));
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    assert_eq!(app.cursor_style(), SetCursorStyle::SteadyBar);

    // Esc → Normal, x 로 끝 글자 삭제
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Quiz);
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    assert_eq!(app.cursor_style(), SetCursorStyle::SteadyBlock);
    type_str(&mut app, "x");
    assert_eq!(texts(&app), vec![answer.clone()]);
    assert_eq!(app.quiz.marks, vec![None]);

    // Normal 의 q 는 무시, j/k 는 코드 스크롤
    type_str(&mut app, "qjk");
    assert!(!app.quiz.follow);
    assert!(!app.quit);
    assert_eq!(texts(&app), vec![answer.clone()]);

    // 0 cw 로 단어 교체 후 Esc, Enter 로 채점
    type_str(&mut app, "0cw");
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    type_str(&mut app, &answer);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.status, Status::Solved);

    // 다음 퀴즈는 다시 Insert 로 시작, Esc 는 Normal 까지만이고 레슨 화면은 Ctrl+O
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Quiz);
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Lesson);
    // 레슨 보기 커서는 블록
    assert_eq!(app.cursor_style(), SetCursorStyle::SteadyBlock);
}

#[test]
fn tab_keeps_mode_across_blanks() {
    let mut app = fixture_app();
    app.open_lesson(1);
    app.start_quiz(1);
    let answer = answers(&app);
    type_str(&mut app, &answer[0]);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        (app.quiz.blank, app.editing_mode()),
        (1, Some(Mode::Normal))
    );
    // Shift+A 로 Insert 진입 후 Tab 하면 다음 빈칸도 Insert (끝에 이어 쓰기)
    app.on_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT));
    type_str(&mut app, &answer[1]);
    press(&mut app, KeyCode::BackTab);
    assert_eq!(
        (app.quiz.blank, app.editing_mode()),
        (0, Some(Mode::Insert))
    );
    type_str(&mut app, "!");
    assert_eq!(
        texts(&app),
        vec![format!("{}!", answer[0]), answer[1].clone()]
    );
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.status, Status::Solved);
}

#[test]
fn all_correct_marks_lesson_done() {
    let mut app = fixture_app();
    let n = app.lessons.len();
    open_quiz(&mut app, 1);
    let total = app.current().quiz.len();
    while app.screen == Screen::Quiz {
        // 마지막 퀴즈는 앞뒤 공백을 붙여도 정답 (공백 무시)
        let last = app.quiz.index + 1 == total;
        for answer in answers(&app) {
            let typed = if last { format!(" {answer} ") } else { answer };
            type_str(&mut app, &typed);
            press(&mut app, KeyCode::Tab);
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.quiz.status, Status::Solved);
        press(&mut app, KeyCode::Enter);
    }
    assert_eq!((app.screen, app.score), (Screen::Result, total));
    assert!(app.done[1]);
    ctrl(&mut app, 'l');
    assert_eq!(app.popup, Some(Popup::Lessons));
    let toc = render(&app, 80, 24);
    let done = format!("{} ✓", app.lessons[1].title);
    assert!(
        toc.contains(&format!(" Lessons 1/{n} ")) && toc.contains(&done),
        "{toc}"
    );
}

#[test]
fn printable_keys_go_to_blank_not_commands() {
    let mut app = fixture_app();
    open_quiz(&mut app, 2);
    type_str(&mut app, "qr?:|&./=>");
    assert!(!app.quit);
    assert_eq!(texts(&app)[0], "qr?:|&./=>");
    assert!(app.pending_run().is_none());

    // 정답에 ? 가 든 퀴즈도 그대로 입력해 맞힌다
    app.start_quiz(0);
    let answer = answers(&app);
    assert!(answer.iter().any(|a| a.contains('?')));
    for answer in answer {
        type_str(&mut app, &answer);
        press(&mut app, KeyCode::Tab);
    }
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.quiz.status, Status::Solved);
}

#[test]
fn ctrl_keys_are_not_typed() {
    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    for c in ['a', 'w', 'x'] {
        ctrl(&mut app, c);
    }
    app.on_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::ALT));
    assert_eq!(texts(&app), vec![""]);
    assert!(vim_key(KeyEvent::new(KeyCode::Char('A'), KeyModifiers::SHIFT)).is_some());
    let mut release = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    release.kind = KeyEventKind::Release;
    assert!(vim_key(release).is_none());
}

#[test]
fn ctrl_r_runs_with_my_answers() {
    let mut app = fixture_app();
    app.open_lesson(1);
    leader(&mut app, 'r');
    assert!(matches!(app.output, Some(Output::Running(_))));
    assert_eq!(app.pending_run(), Some(app.current().example.as_str()));

    quiz_from_start(&mut app);
    let answer = answers(&app)[0].clone();
    type_str(&mut app, &answer);
    run_key(&mut app);
    assert!(matches!(app.output, Some(Output::Running(_))));
    let expected = app.current_quiz().code.replace(BLANK, &answer);
    assert_eq!(app.pending_run(), Some(expected.as_str()));
    assert_eq!(texts(&app), vec![answer]);

    // 채점하면 이전 실행 출력과 패널 스크롤이 초기화되어 해설이 보인다
    app.output = Some(fake_output(false));
    app.panel_scroll.set(5);
    press(&mut app, KeyCode::Enter);
    assert!(app.output.is_none());
    assert_eq!(app.panel_scroll.get(), 0);
    assert!(render(&app, 80, 24).contains("Correct!"));
}

#[test]
fn edit_example_then_run_and_revert() {
    let mut app = fixture_app();
    app.open_lesson(0);
    let original = app.current().example.clone();

    // 편집은 같은 레슨 화면에서 Normal 로 시작, q 로 종료되지 않음
    leader(&mut app, 'e');
    assert_eq!((app.screen, app.editing), (Screen::Lesson, true));
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    press(&mut app, KeyCode::Char('q'));
    assert!(!app.quit && app.popup.is_none());

    // o 로 줄을 열어 입력 → Esc(Normal), Normal 의 Esc 는 편집 그대로 → 목차에서 같은 레슨 Enter(편집 마침)
    press(&mut app, KeyCode::Char('o'));
    type_str(&mut app, "// 수정한 줄");
    assert!(app.modified());
    assert!(render(&app, 80, 24).contains("(modified)"));
    run_key(&mut app);
    let edited = app.pending_run().unwrap().to_string();
    assert!(edited.contains("// 수정한 줄"), "{edited}");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    press(&mut app, KeyCode::Esc);
    assert!(app.editing);
    toc_enter(&mut app);
    assert_eq!((app.screen, app.editing), (Screen::Lesson, false));
    let view = render(&app, 80, 60);
    assert!(view.contains("(modified)"), "{view}");
    assert!(view.contains("│ // 수정한 줄"), "{view}");

    // 보기 모드의 ,r 은 수정본 실행
    leader(&mut app, 'r');
    assert_eq!(app.pending_run(), Some(edited.as_str()));

    // 다시 e 하면 이어서 편집 (u 로 되돌리면 원본과 같아져 수정본이 사라짐)
    leader(&mut app, 'e');
    assert_eq!(app.draft().map(Editor::text), Some(edited));
    press(&mut app, KeyCode::Char('u'));
    ctrl(&mut app, 'o');
    assert!(!app.modified());
    assert!(app.draft().is_none());

    // ,R 은 원본으로 되돌림 (편집 중 Normal 의 R 은 아무 일 없음, 레슨 보기의 R 한 글자도 아무 일 없음)
    leader(&mut app, 'e');
    type_str(&mut app, "dd");
    press(&mut app, KeyCode::Char('R'));
    assert!(app.editing && app.modified());
    ctrl(&mut app, 'o');
    assert!(app.modified());
    press(&mut app, KeyCode::Char('R'));
    assert!(app.modified());
    leader(&mut app, 'R');
    assert!(!app.modified());
    assert!(!render(&app, 80, 24).contains("(modified)"));
    leader(&mut app, 'r');
    assert_eq!(app.pending_run(), Some(original.as_str()));

    // 수정본은 레슨별로 유지된다 (편집 중 목차에서 바로 다른 레슨으로)
    leader(&mut app, 'e');
    type_str(&mut app, "dd");
    ctrl(&mut app, 'l');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        (app.screen, app.lesson, app.editing),
        (Screen::Lesson, 1, false)
    );
    assert!(!app.modified());
    app.open_lesson(0);
    assert!(app.modified());
    ctrl(&mut app, 'c');
    ctrl(&mut app, 'c');
    assert!(app.quit);
}

/// 그린 화면과 보이는 커서 자리 (커서를 숨겼으면 None)
fn screen_cursor(app: &App, w: u16, h: u16) -> (Buffer, Option<Position>) {
    let mut terminal = draw(app, w, h);
    let cursor = terminal
        .backend()
        .cursor_visible()
        .then(|| terminal.get_cursor_position().unwrap());
    (terminal.backend().buffer().clone(), cursor)
}

/// 편집 중인 레슨 화면을 그려 (render 의 폭 검사 포함) 커서가 본문 줄 안에 있는지 검사
fn render_editing(app: &App, w: u16, h: u16) {
    render(app, w, h);
    if let (_, Some(at)) = screen_cursor(app, w, h) {
        assert!(
            at.x < w && (1..h.saturating_sub(1)).contains(&at.y),
            "{w}x{h} {at:?}"
        );
    }
}

/// 화면 텍스트에서 하단 줄을 빼고, 박스 위 테두리 줄(편집 중이면 커서 위치가 붙는 줄)은 ┌ 만 남긴다
fn layout(screen: &str) -> Vec<String> {
    let rows: Vec<&str> = screen.lines().collect();
    rows[..rows.len() - 1]
        .iter()
        .map(|row| match row.starts_with('┌') {
            true => "┌".to_string(),
            false => (*row).to_string(),
        })
        .collect()
}

#[test]
fn editing_keeps_lesson_layout() {
    // e 를 눌러도 머리줄·설명·박스 자리와 접힌 코드가 그대로이고 (박스 높이도 같음),
    // 박스 테두리 색·위치 표시·커서·하단 줄만 바뀐다. Ctrl+O 로 마치면 보기 화면과 똑같다
    let mut app = fixture_app();
    for lesson in [0, 2] {
        for (w, h) in [(80, 24), (80, 60), (40, 12), (13, 30)] {
            for scroll in [0, 3, 7, u16::MAX] {
                app.open_lesson(lesson);
                scroll_to(&app, scroll);
                let (_, view_at) = screen_cursor(&app, w, h);
                let view = render(&app, w, h);
                let offset = app.scroll.get();
                let (first, end) = app.view_lines.get();
                let on_code = app.cursor_line.get().is_some();
                leader(&mut app, 'e');
                let (buf, cursor) = screen_cursor(&app, w, h);
                let at = cursor.expect("편집 중 커서");
                assert!((1..h - 1).contains(&at.y), "{lesson} {w}x{h} {scroll}");
                let row = app.draft().unwrap().cursor().0;
                if first < end {
                    // 보기 커서 자리(예제 위가 아니면 보이던 예제 줄 안)에서 시작해 페이지가 움직이지 않는다
                    if on_code {
                        assert_eq!(
                            view_at.map(|c| c.y),
                            Some(at.y),
                            "{lesson} {w}x{h} {scroll}"
                        );
                    } else {
                        assert!((first..end).contains(&row), "{lesson} {w}x{h} {scroll}");
                    }
                    assert_eq!(app.scroll.get(), offset, "{lesson} {w}x{h} {scroll}");
                    let edit = render(&app, w, h);
                    assert_eq!(layout(&edit), layout(&view), "{lesson} {w}x{h} {scroll}");
                    // 커서는 박스 안쪽, 그 줄의 첫 글자 칸 (커서 열 0)
                    assert_eq!(at.x, 2, "{lesson} {w}x{h} {scroll}");
                    assert_eq!(buf[(0, at.y)].symbol(), "│");
                } else {
                    // 예제가 안 보이던 화면이면 커서 줄이 보이게 페이지가 옮겨 간다
                    assert_eq!(row, 0);
                }
                // 편집을 마치면 보기 커서가 편집기 커서 줄의 박스 안쪽 첫 칸에 남는다
                ctrl(&mut app, 'o');
                let (_, cursor) = screen_cursor(&app, w, h);
                assert_eq!(cursor.map(|c| c.y), Some(at.y), "{lesson} {w}x{h} {scroll}");
                if first < end {
                    assert_eq!(render(&app, w, h), view, "{lesson} {w}x{h} {scroll}");
                }
            }
        }
    }

    // 보기에서 아래로 스크롤해 둔 긴 예제(레슨 3): 화면 밖 위쪽에 있던 커서는 첫 번째로 보이는 줄로,
    // 수정본의 화면 밖 아래쪽 커서는 마지막으로 보이는 줄로 옮긴다
    app.open_lesson(2);
    scroll_to(&app, 6);
    let view = render(&app, 80, 12);
    let (first, _) = app.view_lines.get();
    assert!(first > 0, "{first}");
    leader(&mut app, 'e');
    assert_eq!(app.draft().unwrap().cursor(), (first, 0));
    assert_eq!(layout(&render(&app, 80, 12)), layout(&view));
    let (buf, at) = screen_cursor(&app, 80, 12);
    let at = at.unwrap();
    let line = app.draft().unwrap().lines()[first].trim().to_string();
    assert_eq!(find(&buf, &line).1, at.y);
    type_str(&mut app, "Gx");
    render(&app, 80, 12);
    ctrl(&mut app, 'o');
    assert!(app.modified());
    // 위로 스크롤하면 보기 커서는 화면 맨 아래 줄에 붙는다
    scroll_to(&app, 0);
    render(&app, 80, 12);
    let (_, end) = app.view_lines.get();
    leader(&mut app, 'e');
    assert_eq!(app.draft().unwrap().cursor().0, end - 1);
    assert_eq!(app.scroll.get(), 0);
}

#[test]
fn editing_cursor_sits_on_its_char() {
    // Normal 커서는 접힌 화면에서 커서 글자 칸 위에 있다 (한글 2칸, 박스보다 긴 줄, 탭 포함)
    let mut app = fixture_app();
    app.open_lesson(0);
    let code = "fn main() {\n\tlet 이름 = \"가나다라마바사아자차카타파하 abc\";\t// 탭 뒤 주석\n}";
    app.drafts[0] = Some(Editor::new(code, true, Mode::Normal));
    leader(&mut app, 'e');
    press(&mut app, KeyCode::Char('j'));
    let line: Vec<char> = code.lines().nth(1).unwrap().chars().collect();
    for w in [80, 30, 13, 7] {
        press(&mut app, KeyCode::Char('0'));
        let mut last = None;
        for (i, &c) in line.iter().enumerate() {
            assert_eq!(app.draft().unwrap().cursor(), (1, i));
            let (buf, cursor) = screen_cursor(&app, w, 30);
            let at = cursor.expect("커서");
            let want = if c == '\t' { ' ' } else { c };
            assert_eq!(buf[(at.x, at.y)].symbol(), want.to_string(), "{w} {i}");
            assert_eq!(buf[(w - 1, at.y)].symbol(), "│", "{w} {i}");
            // 페이지 안에서 글자 순서대로 (위→아래, 왼쪽→오른쪽)
            let here = (at.y + app.scroll.get(), at.x);
            assert!(last < Some(here), "{w} {i} {last:?} {here:?}");
            last = Some(here);
            press(&mut app, KeyCode::Char('l'));
        }
    }
    // 탭은 탭 멈춤(4칸)까지 펼쳐 줄 첫 탭 뒤 let 은 박스 안쪽 5번째 칸
    let buf = buffer(&app, 80, 30);
    assert_eq!(find(&buf, "let 이름").0, 2 + 4);
    // 주석 앞 탭(박스 안쪽 50번째 칸)은 다음 탭 멈춤(52)까지
    let (x, y) = find(&buf, "abc\";");
    let tab = x + 5 - 2;
    assert_eq!(tab, 50);
    assert_eq!(find_in_row(&buf, y, "// 탭"), Some(2 + (tab / 4 + 1) * 4));

    // Insert 의 줄 끝 커서는 마지막 글자 바로 뒤, 박스 안쪽이 꽉 찬 줄이면 오른쪽 여백 칸
    // (폭 13 → 안쪽 9칸)
    for (text, x, down) in [("123456789", 11, 0), ("12345678가", 4, 1), ("가", 4, 0)] {
        app.drafts[0] = Some(Editor::new(text, true, Mode::Normal));
        press(&mut app, KeyCode::Char('A'));
        let (buf, at) = screen_cursor(&app, 13, 30);
        let at = at.unwrap();
        let (_, top) = find(&buf, "┌");
        assert_eq!((at.x, at.y), (x, top + 1 + down), "{text}");
        assert_eq!(buf[(12, at.y)].symbol(), "│", "{text}");
        press(&mut app, KeyCode::Esc);
    }
}

#[test]
fn editing_page_follows_cursor() {
    // 긴 예제(레슨 3)를 낮은 화면에서 편집: 커서 줄이 늘 보이도록 설명과 박스가 함께 스크롤되고,
    // 화면 밖 끝 줄·첫 줄로 가면 박스 아래·위 테두리까지 보인다. 머리줄·하단 줄은 그대로
    let (w, h) = (40, 12);
    let mut app = fixture_app();
    app.open_lesson(2);
    let title = format!("3. {}", app.current().title);
    leader(&mut app, 'e');
    type_str(&mut app, "G");
    let (buf, at) = screen_cursor(&app, w, h);
    let at = at.unwrap();
    assert!(app.scroll.get() > 0);
    assert_eq!(buf[(at.x, at.y)].symbol(), "}");
    assert!(find_in_row(&buf, at.y + 1, "└").is_some());
    assert!(find_in_row(&buf, 0, &title).is_some());
    assert!(find_in_row(&buf, h - 1, " NORMAL ").is_some());

    // 위로 한 줄씩: 커서는 늘 본문 안, 첫 줄에 닿으면 위 테두리도 보인다
    let lines = app.draft().unwrap().lines().len();
    for _ in 1..lines {
        press(&mut app, KeyCode::Char('k'));
        render_editing(&app, w, h);
    }
    let (buf, at) = screen_cursor(&app, w, h);
    let at = at.unwrap();
    assert_eq!(app.draft().unwrap().cursor().0, 0);
    assert!(find_in_row(&buf, at.y - 1, "┌ Example").is_some());

    // 박스보다 긴 한글 줄 끝(Insert)으로 가면 접힌 마지막 줄까지 따라간다
    type_str(&mut app, "GA");
    type_str(&mut app, &"가".repeat(40));
    let (buf, at) = screen_cursor(&app, w, h);
    let at = at.unwrap();
    assert_eq!(buf[(at.x - 2, at.y)].symbol(), "가");
    assert_eq!(buf[(at.x, at.y)].symbol(), " ");
    assert!(find_in_row(&buf, at.y + 1, "└").is_some());

    // 편집을 마쳐도 스크롤은 그대로
    let offset = app.scroll.get();
    ctrl(&mut app, 'o');
    render(&app, w, h);
    assert_eq!(app.scroll.get(), offset);
}

#[test]
fn enter_while_editing_is_a_vim_key() {
    // 편집 중 Enter 는 편집기 키 (Normal 다음 줄, Insert 줄바꿈)이고 퀴즈로 가지 않는다
    let mut app = fixture_app();
    app.open_lesson(1);
    leader(&mut app, 'e');
    let lines = app.draft().unwrap().lines().len();
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.screen, app.editing), (Screen::Lesson, true));
    assert_eq!(app.draft().unwrap().cursor().0, 1);
    press(&mut app, KeyCode::Char('o'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.draft().unwrap().lines().len(), lines + 2);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Enter);
    assert_eq!((app.screen, app.editing), (Screen::Lesson, true));
    // 퀴즈는 편집을 마친 뒤 ^O
    ctrl(&mut app, 'o');
    quiz_from_start(&mut app);
    assert_eq!((app.screen, app.quiz.index), (Screen::Quiz, 0));
}

#[test]
fn run_while_editing_shows_output_panel() {
    // 편집 중 ,r(Normal) 은 편집 중인 코드를 실행하고, 결과는 박스 아래 Output 패널.
    // Ctrl+R 은 아무 일 없음. PgUp/PgDn 은 출력 패널 스크롤이고 편집·페이지 스크롤은 그대로
    let mut app = fixture_app();
    app.open_lesson(1);
    leader(&mut app, 'e');
    type_str(&mut app, "o// 실행");
    ctrl(&mut app, 'r');
    assert!(app.pending_run().is_none());
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    run_key(&mut app);
    assert!(app.pending_run().unwrap().contains("// 실행"));
    app.finish_run(RunOutput {
        success: true,
        output: "ok".into(),
    });
    let screen = render(&app, 80, 24);
    assert!(
        screen.contains(" Output ") && screen.contains("Run OK"),
        "{screen}"
    );
    press(&mut app, KeyCode::Esc);
    leader(&mut app, 'r');
    assert!(app.pending_run().unwrap().contains("// 실행"));
    assert_eq!(
        (app.screen, app.editing, app.editing_mode()),
        (Screen::Lesson, true, Some(Mode::Normal))
    );

    app.output = Some(fake_output(false));
    let (buf, at) = screen_cursor(&app, 80, 24);
    let (_, panel) = find(&buf, "┌ Output");
    assert!(at.unwrap().y < panel);
    assert!(find_in_row(&buf, panel, "PgUp/PgDn").is_some());
    let offset = app.scroll.get();
    press(&mut app, KeyCode::PageDown);
    render(&app, 80, 24);
    assert!(app.panel_scroll.get() > 0);
    assert_eq!((app.scroll.get(), app.editing), (offset, true));
    press(&mut app, KeyCode::PageUp);
    render(&app, 80, 24);
    assert_eq!(app.panel_scroll.get(), 0);
}

#[test]
fn hint_toggle_and_navigation() {
    let mut app = fixture_app();
    open_quiz(&mut app, 0);
    show_hint(&mut app);
    assert!(app.quiz.hint);
    assert!(render(&app, 80, 24).contains("Hint:"));
    show_hint(&mut app);
    assert!(!app.quiz.hint);

    // Esc 는 Normal 까지만이고, 레슨 보기는 Ctrl+O, 거기서 q → y 로 종료
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Quiz);
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Lesson);
    press(&mut app, KeyCode::Char('q'));
    press(&mut app, KeyCode::Char('y'));
    assert!(app.quit);
}

#[test]
fn leader_r_runs_and_other_keys_cancel() {
    let mut app = fixture_app();
    app.open_lesson(1);
    // 레슨 보기: r 혼자서는 실행하지 않고, ,r 로 예제 실행
    press(&mut app, KeyCode::Char('r'));
    assert!(app.output.is_none());
    leader(&mut app, 'r');
    assert_eq!(app.pending_run(), Some(app.current().example.as_str()));
    // ,1 ,2 ,3 과 그 밖의 키는 leader 만 취소한다 (Ctrl+L 도 팝업을 열지 않음)
    app.output = None;
    for key in [
        KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
        KeyEvent::new(KeyCode::Char('l'), KeyModifiers::CONTROL),
    ] {
        press(&mut app, KeyCode::Char(','));
        assert!(app.leader);
        app.on_key(key);
        assert!(!app.leader);
        assert_eq!(
            (app.screen, app.lesson, app.popup, app.output.is_none()),
            (Screen::Lesson, 1, None, true),
            "{key:?}"
        );
    }
    // Ctrl+C 와 팝업은 leader 를 취소한다
    press(&mut app, KeyCode::Char(','));
    ctrl(&mut app, 'c');
    assert_eq!((app.popup, app.leader), (Some(Popup::Quit), false));
    press(&mut app, KeyCode::Char('n'));

    // 예제 편집: Insert 의 , 는 입력, Normal 의 ,r 은 편집 중 코드 실행 (편집은 그대로),
    // Normal 의 r, 는 편집기로
    leader(&mut app, 'e');
    type_str(&mut app, "o// ,r");
    assert_eq!(app.draft().unwrap().lines()[1], "// ,r");
    assert!(app.output.is_none());
    press(&mut app, KeyCode::Esc);
    leader(&mut app, 'r');
    assert_eq!(
        (app.screen, app.editing, app.editing_mode()),
        (Screen::Lesson, true, Some(Mode::Normal))
    );
    assert!(app.pending_run().unwrap().contains("// ,r"));
    leader(&mut app, '2');
    assert!(app.editing);
    type_str(&mut app, "0r,");
    assert_eq!(app.draft().unwrap().lines()[1], ",/ ,r");
    // leader 대기 중에는 하단 줄이 다음 키 안내
    press(&mut app, KeyCode::Char(','));
    assert!(foot(&app, 80).starts_with(" LEADER  r run"));
    press(&mut app, KeyCode::Esc);

    // 퀴즈: Insert 의 , 는 입력, Normal 과 잠긴 뒤에는 ,r 로 내 답 실행
    toc_enter(&mut app);
    quiz_from_start(&mut app);
    type_str(&mut app, "a,b");
    assert_eq!((texts(&app), app.leader), (vec!["a,b".to_string()], false));
    assert!(app.output.is_none());
    press(&mut app, KeyCode::Esc);
    leader(&mut app, 'r');
    let code = app.current_quiz().code.clone();
    assert_eq!(app.pending_run(), Some(code.replace(BLANK, "a,b").as_str()));
    leader(&mut app, '3');
    assert_eq!((app.screen, app.quiz.index), (Screen::Quiz, 0));
    show_answer(&mut app);
    leader(&mut app, 'r');
    assert_eq!(
        app.pending_run(),
        Some(code.replace(BLANK, &answers(&app)[0]).as_str())
    );
    leader(&mut app, '1');
    assert_eq!(app.screen, Screen::Quiz);

    // 결과 화면: ,r 은 아무 동작 없이 leader 만 취소
    skip_quizzes(&mut app);
    app.output = None;
    leader(&mut app, 'r');
    assert_eq!(
        (app.screen, app.output.is_none(), app.leader),
        (Screen::Result, true, false)
    );
}

#[test]
fn starts_on_lesson_with_toc_popup() {
    // 인자 없이 시작하면 레슨 1 보기 위에 목차 팝업 (고른 줄 1번), Esc 로 닫으면 레슨 1
    let mut app = fixture_app();
    app.start(None);
    assert_eq!(
        (app.screen, app.lesson, app.popup, app.pick),
        (Screen::Lesson, 0, Some(Popup::Lessons), 0)
    );
    let title = app.lessons[0].title.clone();
    let screen = render(&app, 80, 24);
    assert!(screen.starts_with(&format!("1. {title}")), "{screen}");
    assert!(screen.contains(&format!(">  1. {title}")), "{screen}");
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        (app.screen, app.lesson, app.popup),
        (Screen::Lesson, 0, None)
    );

    // 레슨 번호를 주면 그 레슨 보기, 팝업 없음
    let mut app = fixture_app();
    app.start(Some(2));
    assert_eq!(
        (app.screen, app.lesson, app.popup),
        (Screen::Lesson, 2, None)
    );
}

#[test]
fn toc_popup_fits_screen_with_summaries() {
    let mut app = fixture_app();
    let n = to_u16(app.lessons.len());
    app.start(None);
    for w in [80, 40, 12] {
        let buf = buffer(&app, w, 24);
        let (_, top) = find(&buf, "┌");
        let right = find_in_row(&buf, top, "┐").unwrap();
        // 긴 줄은 끝을 … 로 줄이고, 2칸 문자가 오른쪽 여백·테두리에 걸치지 않는다
        let mut cut = 0;
        for y in top + 1..=top + n {
            assert_eq!(buf[(right, y)].symbol(), "│", "{w} {y}");
            assert!(
                Span::raw(buf[(right - 2, y)].symbol()).width() <= 1,
                "{w} {y}"
            );
            if find_in_row(&buf, y, "…").is_some() {
                cut += 1;
            }
        }
        assert!(cut > 0, "{w}");
    }
    let buf = buffer(&app, 80, 24);
    assert_eq!(buf[find(&buf, "…")].fg, COMMENT);
}

#[test]
fn toc_enter_finishes_edit_or_leaves_quiz() {
    // 예제 편집 중 같은 레슨 Enter: 편집을 마치고(Normal) 출력은 그대로
    let mut app = fixture_app();
    app.open_lesson(1);
    leader(&mut app, 'e');
    type_str(&mut app, "o// 메모");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('d'));
    app.output = Some(fake_output(true));
    toc_enter(&mut app);
    assert_eq!(
        (app.screen, app.lesson, app.popup, app.editing),
        (Screen::Lesson, 1, None, false)
    );
    assert!(app.output.is_some() && app.modified());
    assert_eq!(app.drafts[1].as_ref().unwrap().mode(), Mode::Normal);
    // 수정하지 않은 편집은 버린다
    leader(&mut app, 'R');
    leader(&mut app, 'e');
    toc_enter(&mut app);
    assert!(app.drafts[1].is_none());

    // 퀴즈 중 Enter: 퀴즈를 떠나 레슨 보기 (실행 출력은 비움)
    quiz_from_start(&mut app);
    type_str(&mut app, "mu");
    run_key(&mut app);
    toc_enter(&mut app);
    assert_eq!(
        (app.screen, app.lesson, app.editing_mode()),
        (Screen::Lesson, 1, None)
    );
    assert!(app.output.is_none());
}

#[test]
fn result_enter_on_last_lesson_opens_toc() {
    let mut app = fixture_app();
    let last = app.lessons.len() - 1;
    open_quiz(&mut app, last);
    skip_quizzes(&mut app);
    assert!(render(&app, 80, 24).contains("Enter: lessons"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        (app.screen, app.popup, app.pick),
        (Screen::Result, Some(Popup::Lessons), last)
    );
    press(&mut app, KeyCode::Esc);
    // 목록 화면이 없으므로 결과 화면에서도 q 로 종료 확인
    press(&mut app, KeyCode::Char('q'));
    assert_eq!((app.popup, app.quit), (Some(Popup::Quit), false));
    press(&mut app, KeyCode::Char('y'));
    assert!(app.quit);
}

#[test]
fn ctrl_o_finishes_edit_in_place() {
    let mut app = fixture_app();
    app.open_lesson(1);
    // Insert 에서 한 번에: 같은 화면 그대로 편집만 마치고(Normal) 수정본과 실행 출력·스크롤은 유지
    leader(&mut app, 'e');
    type_str(&mut app, "ix");
    app.output = Some(fake_output(true));
    app.scroll.set(3);
    ctrl(&mut app, 'o');
    assert_eq!(
        (app.screen, app.lesson, app.editing, app.editing_mode()),
        (Screen::Lesson, 1, false, None)
    );
    assert!(app.modified() && app.output.is_some());
    assert_eq!(app.scroll.get(), 3);
    let draft = app.drafts[1].as_ref().unwrap();
    assert_eq!(draft.mode(), Mode::Normal);
    assert!(draft.lines()[0].starts_with('x'));
    // Normal 에서 d 대기 중이어도 이동하고, 다시 편집하면 대기는 사라져 있다
    leader(&mut app, 'e');
    type_str(&mut app, "0d");
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Lesson);
    leader(&mut app, 'e');
    press(&mut app, KeyCode::Char('x'));
    assert!(!app.modified(), "{:?}", app.draft().unwrap().lines()[0]);
    // 원본과 같아진 수정본은 버린다
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Lesson);
    assert!(app.drafts[1].is_none());
}

#[test]
fn ctrl_o_returns_from_quiz_to_lesson() {
    let mut app = fixture_app();
    app.open_lesson(1);
    let back = |app: &mut App| {
        app.output = Some(fake_output(false));
        ctrl(app, 'o');
        assert_eq!(
            (app.screen, app.lesson, app.editing_mode()),
            (Screen::Lesson, 1, None)
        );
        assert!(app.output.is_none());
    };
    // Insert, Normal, Normal 에서 r 대기 중, 잠긴 뒤 모두 한 번에 레슨 보기로
    quiz_from_start(&mut app);
    type_str(&mut app, "mu");
    back(&mut app);
    quiz_from_start(&mut app);
    press(&mut app, KeyCode::Esc);
    back(&mut app);
    quiz_from_start(&mut app);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    back(&mut app);
    quiz_from_start(&mut app);
    assert_eq!(texts(&app), vec![""]);
    show_answer(&mut app);
    back(&mut app);

    // 결과 화면의 ^O 는 이 레슨 보기로
    app.start_quiz(0);
    skip_quizzes(&mut app);
    ctrl(&mut app, 'o');
    assert_eq!(
        (app.screen, app.lesson, app.popup),
        (Screen::Lesson, 1, None)
    );
}

#[test]
fn ctrl_o_toggles_lesson_and_quiz() {
    let mut app = fixture_app();
    app.open_lesson(1);
    // 풀던 퀴즈가 없으면 첫 퀴즈부터, 레슨 보기의 Enter 는 퀴즈로 가지 않는다
    assert!(foot(&app, 80).contains("^O quiz"));
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Lesson);
    ctrl(&mut app, 'o');
    assert_eq!((app.screen, app.quiz.index), (Screen::Quiz, 0));
    // 레슨 보기에 다녀와도 입력·모드 그대로 (하단 줄이 돌아갈 수 있다고 알린다)
    type_str(&mut app, "mu");
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Lesson);
    assert!(
        foot(&app, 80).contains("^O back to quiz"),
        "{}",
        foot(&app, 80)
    );
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Quiz);
    assert_eq!(
        (texts(&app), app.editing_mode()),
        (vec!["mu".into()], Some(Mode::Insert))
    );
    // 대기 중인 r 은 버려져 x 가 바꾸기가 아니라 지우기다 (Normal 유지)
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    ctrl(&mut app, 'o');
    ctrl(&mut app, 'o');
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    press(&mut app, KeyCode::Char('x'));
    assert_eq!(texts(&app), vec!["m"]);
    // 목차로 떠나도 돌아올 수 있다
    toc_enter(&mut app);
    assert_eq!(app.screen, Screen::Lesson);
    ctrl(&mut app, 'o');
    assert_eq!(texts(&app), vec!["m"]);
    // 다른 레슨에서는 그 레슨 퀴즈를 처음부터
    ctrl(&mut app, 'o');
    app.open_lesson(2);
    ctrl(&mut app, 'o');
    assert_eq!(
        (app.screen, app.lesson, app.quiz.index),
        (Screen::Quiz, 2, 0)
    );
    assert!(texts(&app).iter().all(String::is_empty));
    // 예제 편집 중 ^O 는 편집만 마친다
    ctrl(&mut app, 'o');
    leader(&mut app, 'e');
    ctrl(&mut app, 'o');
    assert_eq!((app.screen, app.editing), (Screen::Lesson, false));
}

#[test]
fn esc_never_switches_screens() {
    let mut app = fixture_app();
    let esc = |app: &mut App| {
        let before = (app.screen, app.lesson, app.quiz.index);
        press(app, KeyCode::Esc);
        assert_eq!((app.screen, app.lesson, app.quiz.index), before);
        assert_eq!((app.popup, app.quit), (None, false));
    };
    // 레슨 보기 (종료 확인도 뜨지 않음)
    esc(&mut app);
    app.open_lesson(1);
    esc(&mut app);
    // 예제 편집: Insert → Normal 만, Normal 에서는 그대로 (편집도 끝나지 않음)
    leader(&mut app, 'e');
    press(&mut app, KeyCode::Char('o'));
    esc(&mut app);
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    esc(&mut app);
    esc(&mut app);
    assert!(app.editing);
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    // 퀴즈: Insert → Normal 만, Normal·잠김에서는 그대로 (레슨 보기는 Ctrl+O)
    toc_enter(&mut app);
    quiz_from_start(&mut app);
    esc(&mut app);
    assert_eq!(app.editing_mode(), Some(Mode::Normal));
    esc(&mut app);
    show_answer(&mut app);
    esc(&mut app);
    // 결과
    skip_quizzes(&mut app);
    assert_eq!(app.screen, Screen::Result);
    esc(&mut app);

    // 팝업은 여전히 Esc 로 닫힌다
    for popup in [Popup::Keys, Popup::Lessons, Popup::Quit] {
        app.open(popup);
        press(&mut app, KeyCode::Esc);
        assert_eq!((app.popup, app.screen), (None, Screen::Result), "{popup:?}");
    }
}

#[test]
fn mode_badges_and_borders_use_lualine_colors() {
    // 하단 배지는 lualine(onedark_dark) 처럼 Insert 파랑·Normal 초록·leader 보라 바탕에 검은 굵은 글자,
    // 뒤의 안내 글은 바탕 없음
    let badge = |app: &App, text: &str, bg: Color| {
        let buf = buffer(app, 80, 24);
        let y = buf.area.height - 1;
        assert_eq!(find_in_row(&buf, y, text), Some(0), "{text}");
        let width = to_u16(text.len());
        for x in 0..width {
            let cell = &buf[(x, y)];
            assert_eq!((cell.fg, cell.bg), (BLACK, bg), "{text} {x}");
            assert!(cell.modifier.contains(Modifier::BOLD), "{text} {x}");
        }
        assert_eq!(buf[(width + 1, y)].bg, Color::Reset, "{text}");
    };
    // 편집 중인 코드 박스 테두리는 배지와 같은 모드 색
    let border = |app: &App, label: &str| {
        let buf = buffer(app, 80, 24);
        buf[find(&buf, label)].fg
    };
    // 현재 빈칸은 모드와 상관없이 반전만 (채점 바탕 빨강·초록과 헷갈리지 않게 모드 색을 쓰지 않음)
    let blank = |app: &App| {
        let buf = buffer(app, 80, 24);
        let cell = &buf[pos(&buf, &filled_line(app, 0, "mu"), "mu")];
        assert!(cell.modifier.contains(Modifier::REVERSED));
        (cell.fg, cell.bg)
    };

    let mut app = fixture_app();
    open_quiz(&mut app, 1);
    type_str(&mut app, "mu");
    badge(&app, " INSERT ", BLUE);
    assert_eq!(border(&app, "┌ Rust"), BLUE);
    assert_eq!(blank(&app), (Color::Reset, Color::Reset));
    press(&mut app, KeyCode::Esc);
    badge(&app, " NORMAL ", GREEN);
    assert_eq!(border(&app, "┌ Rust"), GREEN);
    assert_eq!(blank(&app), (Color::Reset, Color::Reset));
    press(&mut app, KeyCode::Char(','));
    badge(&app, " LEADER ", PURPLE);
    press(&mut app, KeyCode::Char('x'));
    // 입력이 잠기면 배지도 모드 색 테두리도 없음
    show_answer(&mut app);
    assert_eq!(border(&app, "┌ Rust"), Color::Reset);
    assert!(foot(&app, 80).starts_with("Enter next"));

    // 예제 편집 박스 테두리도 모드 색 (라벨은 초록 그대로)
    ctrl(&mut app, 'o');
    leader(&mut app, 'e');
    badge(&app, " NORMAL ", GREEN);
    assert_eq!(border(&app, "┌ Example"), GREEN);
    press(&mut app, KeyCode::Char('i'));
    badge(&app, " INSERT ", BLUE);
    assert_eq!(border(&app, "┌ Example"), BLUE);
    // 레슨 보기의 예제 박스는 회색 그대로
    press(&mut app, KeyCode::Esc);
    ctrl(&mut app, 'o');
    assert_eq!(border(&app, "┌ Example"), GRAY);
}

/// bg 색 칸 수
fn bg_cells(buf: &Buffer, bg: Color) -> usize {
    buf.content().iter().filter(|c| c.bg == bg).count()
}

/// 검색 강조 칸 수 (커서 줄의 지금 일치 포함)
fn marked(buf: &Buffer) -> usize {
    bg_cells(buf, SEARCH_BG) + bg_cells(buf, ORANGE)
}

#[test]
fn slash_search_highlights_and_n_scrolls_view() {
    let mut app = fixture_app();
    app.open_lesson(0);
    // 입력하는 동안 하단 줄은 입력 줄이고 일치는 바로 강조된다
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "MAIN");
    assert!(foot(&app, 40).starts_with("/MAIN"));
    assert_eq!(
        marked(&buffer(&app, 40, 12)),
        0,
        "대문자가 있으면 대소문자 구분"
    );
    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace);
    }
    type_str(&mut app, "main");
    assert!(marked(&buffer(&app, 40, 12)) > 0);
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        (app.prompt.as_deref(), app.search.as_deref()),
        (None, Some("main"))
    );
    // n 을 누를 때마다 다음 일치 줄이 주황으로 보이고, 끝에서 처음으로 돈다
    let mut rows = Vec::new();
    for _ in 0..8 {
        let buf = buffer(&app, 40, 12);
        assert!(bg_cells(&buf, ORANGE) > 0, "{}", app.cursor_row.get());
        rows.push(app.cursor_row.get());
        press(&mut app, KeyCode::Char('n'));
    }
    let first = rows[0];
    let cycle = 1 + rows[1..]
        .iter()
        .position(|&r| r == first)
        .expect("돌아오지 않음");
    assert!(rows[..cycle].windows(2).all(|w| w[0] < w[1]), "{rows:?}");
    assert!(cycle >= 3, "{rows:?}");
    // N 은 거꾸로
    buffer(&app, 40, 12);
    let at = app.cursor_row.get();
    press(&mut app, KeyCode::Char('N'));
    buffer(&app, 40, 12);
    assert!(app.cursor_row.get() < at || at == first, "{rows:?} {at:?}");
    // Esc 로 강조를 지운다 (화면은 그대로)
    press(&mut app, KeyCode::Esc);
    let buf = buffer(&app, 40, 12);
    assert_eq!((bg_cells(&buf, SEARCH_BG), bg_cells(&buf, ORANGE)), (0, 0));
    assert_eq!((app.screen, app.search.as_deref()), (Screen::Lesson, None));
}

#[test]
fn slash_search_moves_editor_cursor() {
    let mut app = fixture_app();
    app.open_lesson(1);
    leader(&mut app, 'e');
    let cursor = |app: &App| app.draft().unwrap().cursor();
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "count");
    press(&mut app, KeyCode::Enter);
    assert_eq!(cursor(&app), (3, 12));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(cursor(&app), (4, 4));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(cursor(&app), (5, 15));
    press(&mut app, KeyCode::Char('n'));
    assert_eq!(cursor(&app), (3, 12));
    press(&mut app, KeyCode::Char('N'));
    assert_eq!(cursor(&app), (5, 15));
    // 편집 중에도 일치를 칠한다
    assert!(bg_cells(&buffer(&app, 80, 24), SEARCH_BG) > 0);
    // 비우고 Enter 는 이전 검색어, Esc 와 빈 칸 Backspace 는 취소
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Enter);
    assert_eq!(cursor(&app), (3, 12));
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "x");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('/'));
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        (app.prompt.as_deref(), app.search.as_deref()),
        (None, Some("count"))
    );
    // rn 은 검색이 아니라 n 으로 바꾸기, Insert 에서 / 는 글자
    type_str(&mut app, "rn");
    assert_eq!(app.draft().unwrap().lines()[3], "    let mut nount = 0;");
    type_str(&mut app, "i/");
    assert!(app.prompt.is_none());
    assert_eq!(app.draft().unwrap().lines()[3], "    let mut /nount = 0;");
    // 퀴즈 화면에서는 검색하지 않는다
    press(&mut app, KeyCode::Esc);
    ctrl(&mut app, 'o');
    quiz_from_start(&mut app);
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('/'));
    assert!(app.prompt.is_none());
}

#[test]
fn leader_keys_edit_hint_answer() {
    let mut app = fixture_app();
    app.open_lesson(1);
    // 레슨 보기: e 한 번으로는 편집하지 않고 ,e 로 시작, leader 안내에 e edit
    press(&mut app, KeyCode::Char('e'));
    assert!(!app.editing);
    press(&mut app, KeyCode::Char(','));
    assert!(foot(&app, 80).contains("r run · e edit"));
    press(&mut app, KeyCode::Char('e'));
    assert!(app.editing);
    // 편집 중 ,e 는 아무 일 없음
    let before = app.draft().unwrap().text();
    leader(&mut app, 'e');
    assert!(app.editing);
    assert_eq!(app.draft().unwrap().text(), before);
    ctrl(&mut app, 'o');

    // 퀴즈 Insert: ^T ^E 는 아무 일 없고 , 는 글자
    quiz_from_start(&mut app);
    ctrl(&mut app, 't');
    ctrl(&mut app, 'e');
    assert_eq!((app.quiz.hint, app.quiz.status), (false, Status::Answering));
    type_str(&mut app, ",h");
    assert_eq!(texts(&app), vec![",h"]);
    assert!(!app.quiz.hint);
    // Normal: ,h 힌트 켜고 끄기, ,a 정답 보기 (leader 안내에 h a)
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char(','));
    assert!(foot(&app, 80).contains("h hint · a show solution"));
    press(&mut app, KeyCode::Char('h'));
    assert!(app.quiz.hint);
    leader(&mut app, 'h');
    assert!(!app.quiz.hint);
    leader(&mut app, 'a');
    assert_eq!(app.quiz.status, Status::Revealed);
    assert_eq!(texts(&app), answers(&app));
    // 잠긴 뒤에도 ,h 는 되고 ,a 는 아무 일 없음
    leader(&mut app, 'h');
    assert!(app.quiz.hint);
    leader(&mut app, 'a');
    assert_eq!(app.quiz.status, Status::Revealed);
}

#[test]
fn page_keys_scroll_by_screen() {
    // 레슨 보기: 80x8 이면 본문 6줄 → 한 화면 4줄(두 줄 겹침), 반 화면 3줄
    let mut app = fixture_app();
    app.open_lesson(0);
    let step = |app: &mut App, code: KeyCode, ctrl_key: bool, want: u16| {
        if ctrl_key {
            app.on_key(KeyEvent::new(code, KeyModifiers::CONTROL));
        } else {
            press(app, code);
        }
        render(app, 80, 8);
        assert_eq!(app.scroll.get(), want, "{code:?}");
    };
    render(&app, 80, 8);
    step(&mut app, KeyCode::Char('f'), true, 4);
    step(&mut app, KeyCode::Char('f'), true, 8);
    step(&mut app, KeyCode::Char('b'), true, 4);
    step(&mut app, KeyCode::Char('d'), true, 7);
    step(&mut app, KeyCode::Char('u'), true, 4);
    step(&mut app, KeyCode::PageDown, false, 8);
    step(&mut app, KeyCode::PageUp, false, 4);

    // 예제 편집: ^D/^U/^F/^B 는 커서를 반(한) 화면씩 (열 유지), PgDn 은 출력 패널
    app.open_lesson(1);
    let code: Vec<String> = (0..40).map(|i| format!("let v{i} = {i};")).collect();
    app.drafts[1] = Some(Editor::new(&code.join("\n"), true, Mode::Normal));
    render(&app, 80, 12);
    leader(&mut app, 'e');
    type_str(&mut app, "gglll");
    let cursor = |app: &App| app.draft().unwrap().cursor();
    let page = |app: &mut App, c: char, want: (usize, usize)| {
        ctrl(app, c);
        render(app, 80, 12);
        assert_eq!(cursor(app), want, "^{c}");
    };
    page(&mut app, 'd', (5, 3));
    page(&mut app, 'f', (13, 3));
    page(&mut app, 'u', (8, 3));
    page(&mut app, 'b', (0, 3));
    app.output = Some(fake_output(true));
    render(&app, 80, 12);
    press(&mut app, KeyCode::PageDown);
    assert!(app.panel_scroll.get() > 0);
    assert_eq!(cursor(&app), (0, 3));
    ctrl(&mut app, 'o');

    // 퀴즈: PgDn 과 ^F 는 아래 패널 한 화면, Insert 에서도 글자는 들어가지 않는다
    open_quiz(&mut app, 1);
    app.output = Some(fake_output(false));
    render(&app, 80, 24);
    ctrl(&mut app, 'f');
    let one = app.panel_scroll.get();
    assert!(one > 1);
    ctrl(&mut app, 'b');
    press(&mut app, KeyCode::PageDown);
    assert_eq!(app.panel_scroll.get(), one);
    ctrl(&mut app, 'u');
    assert!(app.panel_scroll.get() > 0 && app.panel_scroll.get() < one);
    assert_eq!(texts(&app), vec![""]);

    // ? 팝업도 한 화면씩
    press(&mut app, KeyCode::Esc);
    question(&mut app);
    render(&app, 80, 12);
    ctrl(&mut app, 'f');
    assert!(app.keys_scroll.get() > 1);
    ctrl(&mut app, 'b');
    assert_eq!(app.keys_scroll.get(), 0);
}

#[test]
fn lesson_view_cursor_moves_and_screen_follows() {
    // 80x8 이면 본문 6줄. j/k 는 커서를 옮기고 화면 끝을 넘을 때만 화면이 따라간다
    let mut app = fixture_app();
    app.open_lesson(0);
    let at = |app: &App| screen_cursor(app, 80, 8).1.expect("보기 커서");
    assert_eq!(at(&app).y, 1);
    for _ in 0..5 {
        press(&mut app, KeyCode::Char('j'));
    }
    assert_eq!(
        (app.cursor_row.get(), app.scroll.get(), at(&app).y),
        (5, 0, 6)
    );
    press(&mut app, KeyCode::Down);
    assert_eq!(
        (app.cursor_row.get(), app.scroll.get(), at(&app).y),
        (6, 1, 6)
    );
    press(&mut app, KeyCode::Char('k'));
    press(&mut app, KeyCode::Up);
    assert_eq!((app.cursor_row.get(), app.scroll.get()), (4, 1));
    // 커서 줄은 CursorLine 바탕
    let buf = buffer(&app, 80, 8);
    assert_eq!((buf[(0, 4)].bg, buf[(79, 4)].bg), (CURSORLINE, CURSORLINE));
    assert_ne!(buf[(79, 3)].bg, CURSORLINE);
    // ^E/^Y 는 화면만 한 줄 (커서가 화면 밖으로 밀리면 끝 줄에 붙는다)
    for _ in 0..4 {
        ctrl(&mut app, 'e');
        render(&app, 80, 8);
    }
    assert_eq!((app.scroll.get(), app.cursor_row.get()), (5, 5));
    ctrl(&mut app, 'y');
    render(&app, 80, 8);
    assert_eq!((app.scroll.get(), app.cursor_row.get()), (4, 5));
    // ^D 는 화면과 커서를 같이 반 화면
    ctrl(&mut app, 'd');
    render(&app, 80, 8);
    assert_eq!((app.scroll.get(), app.cursor_row.get()), (7, 8));
    // 끝에서 j 를 계속 눌러도 커서는 마지막 줄에서 멈춘다
    for _ in 0..200 {
        press(&mut app, KeyCode::Char('j'));
        render(&app, 80, 8);
    }
    let last = app.cursor_row.get();
    assert!(
        buffer(&app, 80, 8)[(0, 6)].symbol() == "└",
        "마지막 줄은 박스 아래 테두리"
    );
    assert_eq!(at(&app).y, 6);
    press(&mut app, KeyCode::Char('j'));
    render(&app, 80, 8);
    assert_eq!(app.cursor_row.get(), last);

    // 예제 줄 위의 커서에서 ,e 하면 그 줄에서 편집을 시작하고, 마치면 커서는 편집기 커서 줄에
    app.open_lesson(1);
    let view = render(&app, 80, 24);
    let row = find(&buffer(&app, 80, 24), "let mut count").1 - 1;
    for _ in 0..row {
        press(&mut app, KeyCode::Char('j'));
    }
    render(&app, 80, 24);
    leader(&mut app, 'e');
    assert_eq!(app.draft().unwrap().cursor(), (3, 0));
    assert_eq!(layout(&render(&app, 80, 24)), layout(&view));
    press(&mut app, KeyCode::Char('j'));
    render(&app, 80, 24);
    ctrl(&mut app, 'o');
    render(&app, 80, 24);
    assert_eq!(app.cursor_row.get(), row + 1);
}

#[test]
fn lesson_view_gg_and_g() {
    let mut app = fixture_app();
    app.open_lesson(0);
    render(&app, 80, 8);
    // G 는 마지막 줄(박스 아래 테두리)로, 화면도 끝까지
    press(&mut app, KeyCode::Char('G'));
    let (buf, at) = screen_cursor(&app, 80, 8);
    assert_eq!(at.map(|c| c.y), Some(6));
    assert_eq!(buf[(0, 6)].symbol(), "└");
    assert!(app.scroll.get() > 0);
    // g 다음 다른 키는 대기를 취소하고 그 키대로, gg 는 맨 위로
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('k'));
    render(&app, 80, 8);
    assert_eq!(screen_cursor(&app, 80, 8).1.map(|c| c.y), Some(5));
    assert!(!app.g_pending);
    type_str(&mut app, "gg");
    render(&app, 80, 8);
    assert_eq!((app.scroll.get(), app.cursor_row.get()), (0, 0));
    // 퀴즈를 다녀오면 g 대기는 사라진다
    press(&mut app, KeyCode::Char('j'));
    press(&mut app, KeyCode::Char('g'));
    ctrl(&mut app, 'o');
    assert_eq!((app.screen, app.g_pending), (Screen::Quiz, false));
}

#[test]
fn review_fixes_quiz_flow() {
    // 마지막 퀴즈에서 ^O 로 다녀와 끝까지 풀면, 같은 레슨의 다음 ^O 는 첫 퀴즈부터 점수 0
    let mut app = fixture_app();
    app.open_lesson(1);
    ctrl(&mut app, 'o');
    while app.quiz.index + 1 < app.current().quiz.len() {
        show_answer(&mut app);
        press(&mut app, KeyCode::Enter);
    }
    ctrl(&mut app, 'o');
    ctrl(&mut app, 'o');
    assert_eq!(app.screen, Screen::Quiz);
    show_answer(&mut app);
    press(&mut app, KeyCode::Enter);
    assert_eq!(app.screen, Screen::Result);
    toc_enter(&mut app);
    assert!(foot(&app, 80).contains("^O quiz"), "{}", foot(&app, 80));
    ctrl(&mut app, 'o');
    assert_eq!(
        (app.screen, app.quiz.index, app.score, app.quiz.status),
        (Screen::Quiz, 0, 0, Status::Answering)
    );

    // 빈칸이 둘 이상이면 Tab 을 팁으로 알리고, Tab 은 대기 중인 r 을 버려 칸 내용이 그대로
    app.start_quiz(1);
    assert!(render(&app, 80, 24).contains(BLANKS_TIP));
    type_str(&mut app, "ab");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('r'));
    press(&mut app, KeyCode::Tab);
    type_str(&mut app, "ix");
    app.on_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(texts(&app), vec!["ab", "x"]);
    assert_eq!(app.editing_mode(), Some(Mode::Insert));
    // Insert 의 ^W 는 단어 지우기
    type_str(&mut app, " cd");
    ctrl(&mut app, 'w');
    assert_eq!(texts(&app)[0], "ab ");
    // Esc 와 , 가 한 번에 와 Alt+, 로 합쳐져도 Esc 다음 leader 로 받는다
    app.on_key(KeyEvent::new(KeyCode::Char(','), KeyModifiers::ALT));
    assert_eq!((app.editing_mode(), app.leader), (Some(Mode::Normal), true));
    press(&mut app, KeyCode::Char('h'));
    assert!(app.quiz.hint);
    // PgDn 은 대기 중인 d 를 버린다
    press(&mut app, KeyCode::Char('d'));
    press(&mut app, KeyCode::PageDown);
    press(&mut app, KeyCode::Char('w'));
    assert_eq!(texts(&app)[0], "ab ");
    // 잠긴 뒤 leader 안내에는 a 가 없다
    show_answer(&mut app);
    press(&mut app, KeyCode::Char(','));
    assert_eq!(foot(&app, 80), format!(" LEADER  {LOCKED_LEADER_HELP}"));
    press(&mut app, KeyCode::Esc);

    // 퀴즈의 Esc 는 레슨 검색 강조를 지우지 않는다
    app.search = Some("let".into());
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.search.as_deref(), Some("let"));
}

#[test]
fn review_fixes_lesson_view() {
    let mut app = fixture_app();
    app.open_lesson(0);
    // ^O 로 퀴즈를 다녀와도 레슨 보기 위치 그대로
    render(&app, 80, 12);
    press(&mut app, KeyCode::Char('G'));
    render(&app, 80, 12);
    let at = (app.scroll.get(), app.cursor_row.get());
    ctrl(&mut app, 'o');
    render(&app, 80, 12);
    ctrl(&mut app, 'o');
    render(&app, 80, 12);
    assert_eq!((app.scroll.get(), app.cursor_row.get()), at);
    // 출력 패널이 생겨도 커서 줄은 그대로이고 화면이 따라간다, 넘친 출력은 스크롤 키를 알린다
    app.output = Some(fake_output(false));
    let (_, cursor) = screen_cursor(&app, 80, 24);
    assert!(cursor.is_some());
    assert_eq!(app.cursor_row.get(), at.1);
    let screen = render(&app, 80, 24);
    assert!(screen.contains(",e then PgUp/PgDn"), "{screen}");
    // 출력 박스는 예제 박스처럼 안쪽 한 칸 띄운다
    app.output = Some(Output::Done(RunOutput {
        success: true,
        output: "[a\tb]".into(),
    }));
    let screen = render(&app, 80, 24);
    assert!(
        screen.contains("│ Run OK") && screen.contains("│ [a  b]"),
        "{screen}"
    );
    app.output = None;

    // 내용이 다 보이면 ^E 는 화면도 커서도 그대로
    press(&mut app, KeyCode::Char('g'));
    press(&mut app, KeyCode::Char('g'));
    render(&app, 80, 60);
    ctrl(&mut app, 'e');
    render(&app, 80, 60);
    assert_eq!((app.scroll.get(), app.cursor_row.get()), (0, 0));

    // 공백이 든 검색어는 줄 끝 뒤 빈 칸과 일치하지 않는다, 없으면 하단 줄에 알린다
    app.open_lesson(1);
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "; ");
    assert_eq!(marked(&buffer(&app, 80, 24)), 0);
    press(&mut app, KeyCode::Enter);
    assert!(foot(&app, 80).ends_with("Pattern not found: ;"));
    press(&mut app, KeyCode::Char('j'));
    assert!(!foot(&app, 80).contains("Pattern not found"));
    // 검색으로 간 일치 칸에서 ,e 하면 그 글자에서 편집 시작
    app.cursor_row.set(0);
    press(&mut app, KeyCode::Char('/'));
    type_str(&mut app, "count");
    press(&mut app, KeyCode::Enter);
    render(&app, 80, 24);
    leader(&mut app, 'e');
    assert_eq!(app.draft().unwrap().cursor(), (3, 12));
    // 마쳤다 다시 ,e 하면 열도 그대로, 편집 중 ^D 는 대기 중인 d 를 버린다
    type_str(&mut app, "$");
    render(&app, 80, 24);
    ctrl(&mut app, 'o');
    render(&app, 80, 24);
    leader(&mut app, 'e');
    assert_eq!(app.draft().unwrap().cursor(), (3, 21));
    type_str(&mut app, "d");
    ctrl(&mut app, 'd');
    type_str(&mut app, "w");
    assert!(!app.modified());
    ctrl(&mut app, 'o');

    // 목차 팝업 gg/G
    ctrl(&mut app, 'l');
    press(&mut app, KeyCode::Char('G'));
    assert_eq!(app.pick, app.lessons.len() - 1);
    type_str(&mut app, "gg");
    assert_eq!(app.pick, 0);
}
