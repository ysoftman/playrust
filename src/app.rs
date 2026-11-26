//! TUI 앱 상태와 키 처리 (바이너리 전용 모듈, lib 에는 없음).
//! 화면 그리기는 `ui`, 색과 블록 모양은 `style`, 2칸 문자 폭·줄바꿈 계산은 `text`.

mod style;
#[cfg(test)]
mod tests;
mod text;
mod ui;

use std::cell::Cell;

use playrust::vim::{Editor, Key, Mode};
use playrust::{Lesson, Quiz, RunOutput, fill_blanks, is_correct};
use ratatui::crossterm::cursor::SetCursorStyle;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use text::{find_all, follow};

pub use ui::ui;

/// 스크롤 위치와, 렌더가 알려 주는 보이는 줄 수 (한 화면 넘기기에 쓴다)
#[derive(Default)]
struct Scroll {
    offset: Cell<u16>,
    height: Cell<u16>,
    // 렌더가 알려 주는 가장 아래 위치 (^E 가 끝에서 더 내려가지 않게)
    max: Cell<u16>,
}

impl Scroll {
    fn get(&self) -> u16 {
        self.offset.get()
    }

    fn set(&self, offset: u16) {
        self.offset.set(offset);
    }

    fn by(&self, delta: i16) {
        self.set(self.get().saturating_add_signed(delta));
    }

    /// vim 처럼 한 화면은 두 줄 겹치게, 반 화면은 절반 (최소 1줄)
    fn page_size(&self, half: bool) -> u16 {
        let height = self.height.get();
        if half {
            height / 2
        } else {
            height.saturating_sub(2)
        }
        .max(1)
    }

    /// 넘긴 줄 수(위로는 음수)를 돌려준다
    fn page(&self, down: bool, half: bool) -> i16 {
        let n = i16::try_from(self.page_size(half)).unwrap_or(i16::MAX);
        let n = if down { n } else { -n };
        self.by(n);
        n
    }
}

/// PgUp/PgDn 과 vim 의 ^B/^F (한 화면), ^U/^D (반 화면) → (아래로, 반 화면)
fn page_key(code: KeyCode, ctrl: bool) -> Option<(bool, bool)> {
    Some(match code {
        KeyCode::PageUp => (false, false),
        KeyCode::PageDown => (true, false),
        KeyCode::Char('b') if ctrl => (false, false),
        KeyCode::Char('f') if ctrl => (true, false),
        KeyCode::Char('u') if ctrl => (false, true),
        KeyCode::Char('d') if ctrl => (true, true),
        _ => return None,
    })
}

/// 예제 편집은 화면이 아니라 레슨 화면 안의 상태다 (`App::editing`)
#[derive(Clone, Copy, Debug, PartialEq)]
enum Screen {
    Lesson,
    Quiz,
    Result,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Status {
    Answering,
    Solved,
    Revealed,
}

enum Output {
    /// 실행할 코드. 메인 루프가 "Running…" 을 그린 뒤 실행해 Done 으로 바꾼다
    Running(String),
    Done(RunOutput),
}

/// 화면 위에 띄우는 팝업 (한 번에 하나)
#[derive(Clone, Copy, Debug, PartialEq)]
enum Popup {
    Keys,
    Quit,
    Lessons,
}

/// 풀고 있는 퀴즈 하나의 입력·채점 상태
struct QuizRun {
    /// 현재 레슨의 몇 번째 퀴즈인지
    index: usize,
    inputs: Vec<Editor>,
    marks: Vec<Option<bool>>,
    blank: usize,
    status: Status,
    hint: bool,
    /// 코드가 현재 빈칸을 따라 스크롤할지 (↑/↓ 로 직접 스크롤하면 해제)
    follow: bool,
}

impl QuizRun {
    /// 빈칸 blanks 개를 비워 두고 Insert 로 시작
    fn new(index: usize, blanks: usize) -> Self {
        Self {
            index,
            inputs: (0..blanks)
                .map(|_| Editor::new("", false, Mode::Insert))
                .collect(),
            marks: vec![None; blanks],
            blank: 0,
            status: Status::Answering,
            hint: false,
            follow: true,
        }
    }

    /// 입력 중인 빈칸의 편집기 (맞히거나 정답을 본 뒤에는 None)
    fn editor(&self) -> Option<&Editor> {
        match self.status {
            Status::Answering => self.inputs.get(self.blank),
            Status::Solved | Status::Revealed => None,
        }
    }

    /// 현재 빈칸 편집기에 키를 넘긴다. false 면 앱이 처리할 키
    fn edit_blank(&mut self, key: KeyEvent) -> bool {
        let (Some(k), Some(editor)) = (vim_key(key), self.inputs.get_mut(self.blank)) else {
            return false;
        };
        let before = editor.text();
        if !editor.key(k) {
            return false;
        }
        if editor.text() != before {
            self.marks[self.blank] = None;
        }
        self.follow = true;
        true
    }

    /// 빈칸을 옮기며 Insert/Normal 모드를 그대로 이어 간다. 떠나는 칸과 가는 칸의 대기 중인
    /// 연산자(r, d 등)는 취소해 모드를 잇는 키가 그 연산자에 먹히지 않게 한다
    fn select_blank(&mut self, i: usize) {
        let Some(leaving) = self.inputs.get_mut(self.blank) else {
            return;
        };
        leaving.cancel_pending();
        let mode = leaving.mode();
        self.blank = i % self.inputs.len();
        self.follow = true;
        let editor = &mut self.inputs[self.blank];
        editor.cancel_pending();
        let key = match (mode, editor.mode()) {
            (Mode::Insert, Mode::Normal) => Key::Char('A'),
            (Mode::Normal, Mode::Insert) => Key::Esc,
            _ => return,
        };
        editor.key(key);
    }
}

pub struct App {
    lessons: Vec<Lesson>,
    done: Vec<bool>,
    // 레슨별 예제 수정본 (세션 동안 유지, 보기 모드에서는 원본과 다를 때만 남긴다)
    drafts: Vec<Option<Editor>>,
    screen: Screen,
    lesson: usize,
    // 레슨 화면의 예제 박스 안에서 바로 편집하는 중 (Screen::Lesson 에서만, 편집기는 drafts 의 수정본)
    editing: bool,
    // 렌더 시 실제 내용 길이에 맞춰 보정되므로 Cell. 편집 중에는 렌더가 커서를 따라 옮긴다
    scroll: Scroll,
    // 마지막으로 그린 레슨 화면에 첫 줄이 보이던 예제 줄 범위 [처음, 끝).
    // 보기 커서가 예제 밖일 때 편집을 시작하면 커서를 이 안으로 옮겨 화면이 움직이지 않게 한다
    view_lines: Cell<(usize, usize)>,
    // 레슨 보기 커서의 페이지 줄 (j/k 로 움직이고 화면이 따라간다. 렌더가 내용 끝으로 자른다)
    cursor_row: Cell<u16>,
    // 레슨 보기 커서의 칸 (n/N 으로 간 일치의 첫 칸, 줄을 옮기면 None 이고 줄 첫 칸에 그린다)
    cursor_x: Cell<Option<u16>>,
    // 보기 커서가 놓인 예제 (줄, 그 접힌 줄의 첫 글자). 예제 박스 안일 때만이고 편집을 여기서 시작한다
    cursor_line: Cell<Option<(usize, usize)>>,
    panel_scroll: Scroll,
    // 퀴즈 코드 스크롤 (레슨 보기 위치와 따로 둬 ^O 로 오가도 서로 유지된다)
    quiz_scroll: Scroll,
    output: Option<Output>,
    quiz: QuizRun,
    score: usize,
    // 퀴즈를 풀다 레슨 보기로 온 레슨. 그 레슨에서 ^O 를 누르면 처음부터가 아니라 그 퀴즈로 돌아간다
    paused: Option<usize>,
    popup: Option<Popup>,
    keys_scroll: Scroll,
    // 목차 팝업에서 고른 레슨
    pick: usize,
    // leader(`,`) 를 눌러 다음 키(r 실행)를 기다리는 중
    leader: bool,
    // 레슨 보기에서 g 를 눌러 다음 g(gg) 를 기다리는 중
    g_pending: bool,
    // `/` 를 누른 뒤 하단 줄에 입력 중인 검색어
    prompt: Option<String>,
    // 레슨 화면에서 강조하는 검색어 (n/N 으로 이동, Esc 로 지움)
    search: Option<String>,
    // 레슨 보기의 검색 이동: 렌더가 일치를 찾은 뒤 커서를 옮긴다
    // (다음이면 true, 커서 줄의 일치도 치면 true: Enter 는 커서 줄부터, n/N 은 그 다음부터)
    jump: Cell<Option<(bool, bool)>>,
    // 마지막 검색 이동에서 일치를 못 찾았다 (다음 키까지 하단 줄에 알린다)
    not_found: Cell<bool>,
    quit: bool,
}

impl App {
    pub fn new(lessons: Vec<Lesson>) -> Self {
        Self {
            done: vec![false; lessons.len()],
            drafts: lessons.iter().map(|_| None).collect(),
            lessons,
            screen: Screen::Lesson,
            lesson: 0,
            editing: false,
            scroll: Scroll::default(),
            view_lines: Cell::new((0, 0)),
            cursor_row: Cell::new(0),
            cursor_x: Cell::new(None),
            cursor_line: Cell::new(None),
            panel_scroll: Scroll::default(),
            quiz_scroll: Scroll::default(),
            output: None,
            quiz: QuizRun::new(0, 0),
            score: 0,
            paused: None,
            popup: None,
            keys_scroll: Scroll::default(),
            pick: 0,
            leader: false,
            g_pending: false,
            prompt: None,
            search: None,
            jump: Cell::new(None),
            not_found: Cell::new(false),
            quit: false,
        }
    }

    /// 시작 화면: 레슨 번호(0부터)를 주면 그 레슨 보기, 없으면 레슨 1 보기 위에 목차 팝업
    pub fn start(&mut self, lesson: Option<usize>) {
        match lesson {
            Some(i) => self.open_lesson(i),
            None => self.open(Popup::Lessons),
        }
    }

    pub fn quitting(&self) -> bool {
        self.quit
    }

    fn current(&self) -> &Lesson {
        &self.lessons[self.lesson]
    }

    fn current_quiz(&self) -> &Quiz {
        &self.current().quiz[self.quiz.index]
    }

    fn draft(&self) -> Option<&Editor> {
        self.drafts[self.lesson].as_ref()
    }

    fn modified(&self) -> bool {
        self.draft()
            .is_some_and(|d| d.text().trim_end() != self.current().example.trim_end())
    }

    /// 수정본이 있으면 수정본, 없으면 원본 예제
    fn example_code(&self) -> String {
        self.draft()
            .map_or_else(|| self.current().example.clone(), Editor::text)
    }

    /// 화면 전환은 모두 여기를 거친다. 예제를 편집 중이면 편집을 마무리하고, 그대로 레슨 화면에
    /// 남으면(목차에서 같은 레슨 Enter) 실행 출력을 유지한다. 그 밖에는 출력을 비운다.
    /// 레슨 보기 위치는 레슨이 바뀔 때만 처음으로 (퀴즈를 다녀와도 보던 자리 그대로)
    fn goto(&mut self, screen: Screen) {
        self.g_pending = false;
        if self.screen == Screen::Quiz && screen == Screen::Lesson {
            // 퀴즈는 그대로 남겨 ^O 로 돌아온다. 대기 중인 키(r, d 등)만 버리고 Insert/Normal 은 유지
            self.paused = Some(self.lesson);
            if let Some(e) = self.quiz.inputs.get_mut(self.quiz.blank) {
                e.cancel_pending();
            }
        }
        self.screen = screen;
        if self.stop_editing() && screen == Screen::Lesson {
            return;
        }
        self.clear_output();
    }

    /// i 번 레슨 보기로 간다. 다른 레슨이면 처음부터 열고, 같은 레슨이면 goto 규칙대로
    /// (예제 편집 중이면 편집만 마치고 출력·스크롤 유지, 그 밖에는 비움)
    fn open_lesson(&mut self, i: usize) {
        self.goto(Screen::Lesson);
        if i != self.lesson {
            self.lesson = i;
            self.reset_view();
        }
    }

    fn start_quiz(&mut self, n: usize) {
        self.quiz = QuizRun::new(n, self.current().quiz[n].answers.len());
        self.quiz_scroll.set(0);
        self.paused = None;
        self.goto(Screen::Quiz);
    }

    /// 실행은 메인 루프가 "Running…" 을 그린 뒤에 한다 (pending_run → finish_run)
    fn request_run(&mut self, code: String) {
        self.output = Some(Output::Running(code));
        self.panel_scroll.set(0);
    }

    /// 실행을 기다리는 코드
    pub fn pending_run(&self) -> Option<&str> {
        match &self.output {
            Some(Output::Running(code)) => Some(code),
            _ => None,
        }
    }

    pub fn finish_run(&mut self, run: RunOutput) {
        self.output = Some(Output::Done(run));
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // Esc 바로 뒤의 글자가 한 번에 오면 터미널이 Alt+글자로 합친다: Esc 와 그 글자로 나눈다
        if key.modifiers.contains(KeyModifiers::ALT)
            && let KeyCode::Char(c) = key.code
        {
            self.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
            let rest = key.modifiers.difference(KeyModifiers::ALT);
            return self.on_key(KeyEvent::new(KeyCode::Char(c), rest));
        }
        self.not_found.set(false);
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        // 종료 확인 중에 Ctrl+C 를 한 번 더 누르면 바로 종료
        if ctrl && key.code == KeyCode::Char('c') {
            if self.popup == Some(Popup::Quit) {
                self.quit = true;
            } else {
                self.open(Popup::Quit);
            }
            return;
        }
        if self.prompt.is_some() {
            return self.prompt_key(key.code, ctrl);
        }
        if let Some(popup) = self.popup {
            match (popup, key.code) {
                (Popup::Lessons, KeyCode::Char('l')) if ctrl => self.popup = None,
                (Popup::Keys, code) => self.keys_key(code, ctrl),
                _ if ctrl => {}
                (Popup::Quit, code) => self.quit_key(code),
                (Popup::Lessons, code) => self.lessons_key(code),
            }
            return;
        }
        let plain = key.modifiers.difference(KeyModifiers::SHIFT).is_empty();
        // leader 다음 키는 r 이면 실행, 그 밖의 키는 vim 처럼 취소만 한다
        if std::mem::take(&mut self.leader) {
            if plain {
                self.leader_key(key.code);
            }
            return;
        }
        if ctrl && key.code == KeyCode::Char('l') {
            return self.open(Popup::Lessons);
        }
        let handled = match self.screen {
            Screen::Lesson if self.editing => self.edit_key(key, ctrl),
            Screen::Lesson => self.lesson_key(key.code, ctrl),
            Screen::Quiz => self.quiz_key(key, ctrl),
            Screen::Result => self.result_key(key.code, ctrl),
        };
        // 화면과 편집기가 쓰지 않은 키만 전역 키로 본다
        // (Insert 와 `r?` 처럼 다음 키를 기다릴 때는 편집기가 글자로 받는다)
        if !handled && plain {
            match key.code {
                KeyCode::Char('?') => self.open(Popup::Keys),
                // 결과 화면에는 leader 로 할 일이 없다
                KeyCode::Char(',') if self.screen != Screen::Result => self.leader = true,
                KeyCode::Char('/') if self.screen == Screen::Lesson => {
                    self.prompt = Some(String::new());
                }
                KeyCode::Char(c @ ('n' | 'N')) if self.screen == Screen::Lesson => {
                    self.search_from(c == 'n', false);
                }
                KeyCode::Esc if self.screen == Screen::Lesson => self.search = None,
                _ => {}
            }
        }
    }

    fn open(&mut self, popup: Popup) {
        self.popup = Some(popup);
        self.leader = false;
        self.g_pending = false;
        self.prompt = None;
        self.keys_scroll.set(0);
        self.pick = self.lesson;
    }

    /// `/` 검색어 입력: Enter 로 찾고 (비우고 Enter 면 이전 검색어로), Esc 나 빈 칸에서
    /// Backspace 는 취소. 입력하는 동안에도 렌더가 입력 중인 검색어를 강조한다
    fn prompt_key(&mut self, code: KeyCode, ctrl: bool) {
        let Some(query) = self.prompt.as_mut() else {
            return;
        };
        match code {
            KeyCode::Char(c) if !ctrl => query.push(c),
            KeyCode::Backspace if !query.is_empty() => {
                query.pop();
            }
            KeyCode::Backspace | KeyCode::Esc => self.prompt = None,
            KeyCode::Enter => {
                let query = self.prompt.take().unwrap_or_default();
                if !query.is_empty() {
                    self.search = Some(query);
                }
                self.search_from(true, true);
            }
            _ => {}
        }
    }

    /// 검색 이동. 예제 편집 중이면 편집기 커서를 다음(이전) 일치로 옮기고 (끝에서 처음으로 돈다),
    /// 보기에서는 렌더가 일치를 찾은 뒤 보기 커서를 일치 줄로 옮긴다 (inclusive 면 커서 줄부터)
    fn search_from(&mut self, forward: bool, inclusive: bool) {
        let Some(query) = self.search.as_ref().map(|q| q.chars().collect::<Vec<_>>()) else {
            return;
        };
        if !self.editing {
            self.jump.set(Some((forward, inclusive)));
            return;
        }
        let Some(editor) = self.drafts[self.lesson].as_mut() else {
            return;
        };
        let hits: Vec<(usize, usize)> = editor
            .lines()
            .iter()
            .enumerate()
            .flat_map(|(r, line)| {
                let chars: Vec<char> = line.chars().collect();
                find_all(&chars, &query).into_iter().map(move |c| (r, c))
            })
            .collect();
        let at = editor.cursor();
        let next = if forward {
            hits.iter().find(|&&h| h > at).or(hits.first())
        } else {
            hits.iter().rev().find(|&&h| h < at).or(hits.last())
        };
        match next {
            Some(&(row, col)) => editor.set_cursor(row, col),
            None => self.not_found.set(true),
        }
    }

    /// leader(`,`) 다음 키: r 실행, 레슨 보기에서는 e 예제 편집·R 원본 복원, 퀴즈에서는 h 힌트,
    /// a 정답 보기.
    /// 습관적으로 누른 한 글자에 실행되거나 정답이 보이지 않게 leader 를 거친다
    /// (Insert 에서는 `,` 가 글자로 입력되므로 Esc 로 나온 뒤에 쓴다)
    fn leader_key(&mut self, code: KeyCode) {
        let quiz = self.screen == Screen::Quiz;
        match code {
            KeyCode::Char('r') => self.run_current(),
            KeyCode::Char('e') if self.screen == Screen::Lesson && !self.editing => {
                self.edit_example();
            }
            // 수정본을 버리는 일이라 한 글자가 아니라 leader 로
            KeyCode::Char('R') if self.screen == Screen::Lesson && !self.editing => {
                self.drafts[self.lesson] = None;
            }
            KeyCode::Char('h') if quiz => self.quiz.hint = !self.quiz.hint,
            KeyCode::Char('a') if quiz && self.quiz.status == Status::Answering => self.reveal(),
            _ => {}
        }
    }

    /// 지금 화면의 코드를 실행한다: 레슨 화면(보기·편집 중)은 예제(수정본이 있으면 수정본),
    /// 퀴즈는 내 답을 채운 코드. 결과 화면에서는 아무것도 하지 않는다
    fn run_current(&mut self) {
        let code = match self.screen {
            Screen::Lesson => self.example_code(),
            Screen::Quiz => fill_blanks(&self.current_quiz().code, |i| {
                self.quiz
                    .inputs
                    .get(i - 1)
                    .map(Editor::text)
                    .unwrap_or_default()
            }),
            Screen::Result => return,
        };
        self.request_run(code);
    }

    /// 레슨 보기의 ^O: 이 레슨 퀴즈를 풀다 왔으면 그 자리로, 아니면 첫 퀴즈부터 점수 0 으로
    fn resume_quiz(&mut self) {
        if self.paused == Some(self.lesson) {
            // 돌아온 퀴즈는 다시 떠날 때 goto 가 표시한다 (다 풀면 다음 ^O 는 처음부터)
            self.goto(Screen::Quiz);
            self.paused = None;
            self.quiz.follow = true;
        } else if !self.current().quiz.is_empty() {
            self.score = 0;
            self.start_quiz(0);
        }
    }

    fn quit_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('y' | 'Y') | KeyCode::Enter => self.quit = true,
            KeyCode::Char('n' | 'N' | 'q') | KeyCode::Esc => self.popup = None,
            _ => {}
        }
    }

    /// 목차 팝업: Enter 면 고른 레슨 화면으로
    fn lessons_key(&mut self, code: KeyCode) {
        let gg = std::mem::take(&mut self.g_pending) && code == KeyCode::Char('g');
        match code {
            _ if gg => self.pick = 0,
            KeyCode::Char('g') => self.g_pending = true,
            KeyCode::Char('G') => self.pick = self.lessons.len() - 1,
            KeyCode::Up | KeyCode::Char('k') => {
                self.pick = step(self.pick, true, self.lessons.len());
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.pick = step(self.pick, false, self.lessons.len());
            }
            KeyCode::Enter => {
                self.popup = None;
                self.open_lesson(self.pick);
            }
            KeyCode::Char('q') | KeyCode::Esc => self.popup = None,
            _ => {}
        }
    }

    fn keys_key(&mut self, code: KeyCode, ctrl: bool) {
        if let Some((down, half)) = page_key(code, ctrl) {
            self.keys_scroll.page(down, half);
            return;
        }
        if ctrl {
            return;
        }
        match code {
            KeyCode::Char('?' | 'q') | KeyCode::Esc | KeyCode::Enter => self.popup = None,
            KeyCode::Up | KeyCode::Char('k') => self.keys_scroll.by(-1),
            KeyCode::Down | KeyCode::Char('j') => self.keys_scroll.by(1),
            _ => {}
        }
    }

    // 화면별 키 처리는 처리한 키면 true 를 돌려준다 (false 면 on_key 가 전역 키로 본다)

    fn lesson_key(&mut self, code: KeyCode, ctrl: bool) -> bool {
        // g 다음 키: g 면 맨 위로, 그 밖의 키는 대기를 취소하고 그 키대로
        if std::mem::take(&mut self.g_pending) && code == KeyCode::Char('g') && !ctrl {
            self.scroll.set(0);
            self.cursor_row.set(0);
            self.cursor_x.set(None);
            return true;
        }
        // 화면을 넘기면 커서도 같은 줄 수만큼 (vim 의 ^D/^U 처럼 화면 안 자리를 유지)
        if let Some((down, half)) = page_key(code, ctrl) {
            let n = self.scroll.page(down, half);
            self.cursor_row
                .set(self.cursor_row.get().saturating_add_signed(n));
            self.cursor_x.set(None);
            return true;
        }
        match code {
            KeyCode::Char('o') if ctrl => self.resume_quiz(),
            // vim 의 ^E/^Y: 커서는 두고 화면만 한 줄 (커서가 화면 밖으로 나가면 끝 줄에 붙인다)
            KeyCode::Char(c @ ('e' | 'y')) if ctrl => {
                let top = match c {
                    'e' => (self.scroll.get() + 1).min(self.scroll.max.get()),
                    _ => self.scroll.get().saturating_sub(1),
                };
                self.scroll.set(top);
                let bottom = top.saturating_add(self.scroll.height.get().max(1) - 1);
                let row = self.cursor_row.get();
                if !(top..=bottom).contains(&row) {
                    self.cursor_row.set(row.clamp(top, bottom));
                    self.cursor_x.set(None);
                }
            }
            _ if ctrl => return false,
            KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
            KeyCode::Char('g') => self.g_pending = true,
            // 맨 아래로 (렌더가 내용 끝으로 자른다)
            KeyCode::Char('G') => {
                self.scroll.set(u16::MAX);
                self.cursor_row.set(u16::MAX);
                self.cursor_x.set(None);
            }
            KeyCode::Char('q') => self.open(Popup::Quit),
            _ => return false,
        }
        true
    }

    /// 레슨 보기 커서를 delta 줄 옮기고 화면 밖으로 나가면 화면이 따라간다
    /// (내용 끝은 렌더가 자른다)
    fn move_cursor(&mut self, delta: i16) {
        let row = self.cursor_row.get().saturating_add_signed(delta);
        self.cursor_row.set(row);
        self.cursor_x.set(None);
        self.scroll
            .set(follow(self.scroll.get(), row, self.scroll.height.get()));
    }

    /// 화면을 바꾸지 않고 레슨 화면의 예제 박스 안에서 편집을 시작한다 (수정본이 있으면 이어서).
    /// 커서가 화면 밖 줄에 있으면 vim 처럼 보이는 줄 안으로 옮겨 페이지가 움직이지 않게 한다
    fn edit_example(&mut self) {
        let example = self.current().example.trim_end().to_string();
        let editor = self.drafts[self.lesson]
            .get_or_insert_with(|| Editor::new(&example, true, Mode::Normal));
        // 보기 커서가 예제 위면 그 자리에서, 아니면 보이는 예제 줄 안에서 시작
        let (first, end) = self.view_lines.get();
        let row = editor.cursor().0;
        if let Some((line, col)) = self.cursor_line.get() {
            editor.set_cursor(line, col);
        } else if first < end {
            let target = row.clamp(first, end - 1);
            let key = if target < row { Key::Up } else { Key::Down };
            for _ in 0..row.abs_diff(target) {
                editor.key(key);
            }
        }
        self.editing = true;
    }

    /// 예제 편집 중인 키. Enter 도 편집기 키이고, Normal 에서 매핑 없는 키
    /// (`q`, `R`, Esc 등)는 편집기가 false 를 돌려줘 아무 일도 없다
    fn edit_key(&mut self, key: KeyEvent, ctrl: bool) -> bool {
        // PgUp/PgDn 은 실행 출력 패널, vim 의 ^F/^B/^D/^U 는 커서를 한(반) 화면씩
        // (d 등 대기 중인 연산자는 취소한다)
        if let Some((down, half)) = page_key(key.code, ctrl) {
            if let Some(editor) = self.drafts[self.lesson].as_mut() {
                editor.cancel_pending();
            }
            if ctrl {
                self.page_cursor(down, half);
            } else {
                self.panel_scroll.page(down, half);
            }
            return true;
        }
        match key.code {
            // 편집만 마치고 같은 화면 그대로 (실행 출력·스크롤 유지)
            KeyCode::Char('o') if ctrl => {
                self.stop_editing();
            }
            // ^W 는 Insert 의 단어 지우기라 편집기로
            _ if ctrl && key.code != KeyCode::Char('w') => return false,
            _ => {
                return vim_key(key)
                    .zip(self.drafts[self.lesson].as_mut())
                    .is_some_and(|(k, editor)| editor.key(k));
            }
        }
        true
    }

    /// 예제 편집 중 ^F/^B/^D/^U: vim 처럼 커서를 화면 줄 수(반 화면은 절반)만큼 위아래로
    /// 옮긴다 (열은 유지, 화면은 커서를 따라간다)
    fn page_cursor(&mut self, down: bool, half: bool) {
        let n = usize::from(self.scroll.page_size(half));
        if let Some(editor) = self.drafts[self.lesson].as_mut() {
            let (row, col) = editor.cursor();
            let row = if down { row + n } else { row.saturating_sub(n) };
            editor.set_cursor(row, col);
        }
    }

    /// 예제 편집을 마친다: Normal 로 돌리고 (대기 중인 연산자도 취소), 원본과 같으면 수정본을 버린다.
    /// 편집 중이었으면 true
    fn stop_editing(&mut self) -> bool {
        if !std::mem::take(&mut self.editing) {
            return false;
        }
        if let Some(editor) = self.drafts[self.lesson].as_mut() {
            editor.key(Key::Esc);
        }
        if !self.modified() {
            self.drafts[self.lesson] = None;
        }
        true
    }

    fn quiz_key(&mut self, key: KeyEvent, ctrl: bool) -> bool {
        let answering = self.quiz.status == Status::Answering;
        // PgUp/PgDn 과 ^F/^B/^D/^U 는 아래 패널(힌트·해설·실행 출력)
        // (d 등 대기 중인 연산자는 취소한다)
        if let Some((down, half)) = page_key(key.code, ctrl) {
            if let Some(editor) = self.quiz.inputs.get_mut(self.quiz.blank) {
                editor.cancel_pending();
            }
            self.panel_scroll.page(down, half);
            return true;
        }
        match key.code {
            // 어느 모드에서든 레슨 보기로 (레슨 보기에서 ^O 를 다시 누르면 이 퀴즈로 돌아온다)
            KeyCode::Char('o') if ctrl => self.goto(Screen::Lesson),
            // ^W 는 Insert 의 단어 지우기라 편집기로
            _ if ctrl && key.code != KeyCode::Char('w') => return false,
            KeyCode::Tab => self.quiz.select_blank(self.quiz.blank + 1),
            KeyCode::BackTab => self
                .quiz
                .select_blank(self.quiz.blank + self.quiz.inputs.len().saturating_sub(1)),
            _ if answering && self.quiz.edit_blank(key) => {}
            KeyCode::Enter if answering => self.check(),
            KeyCode::Enter => self.next_quiz(),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_code(-1),
            KeyCode::Down | KeyCode::Char('j') => self.scroll_code(1),
            _ => return false,
        }
        true
    }

    fn scroll_code(&mut self, delta: i16) {
        self.quiz.follow = false;
        self.quiz_scroll.by(delta);
    }

    /// 화면 스크롤과 실행 출력을 처음 상태로
    fn reset_view(&mut self) {
        self.scroll.set(0);
        self.cursor_row.set(0);
        self.cursor_x.set(None);
        self.view_lines.set((0, 0));
        self.clear_output();
    }

    /// 채점·정답 보기 결과가 가려지지 않게 이전 실행 출력은 지운다
    fn clear_output(&mut self) {
        self.output = None;
        self.panel_scroll.set(0);
    }

    fn check(&mut self) {
        self.clear_output();
        self.quiz.marks = self
            .quiz
            .inputs
            .iter()
            .zip(&self.current_quiz().answers)
            .map(|(input, answer)| Some(is_correct(&input.text(), answer)))
            .collect();
        match self.quiz.marks.iter().position(|m| *m == Some(false)) {
            Some(wrong) => self.quiz.select_blank(wrong),
            None => {
                self.quiz.status = Status::Solved;
                self.score += 1;
            }
        }
    }

    fn reveal(&mut self) {
        self.clear_output();
        self.quiz.inputs = self
            .current_quiz()
            .answers
            .iter()
            .map(|answer| Editor::new(answer, false, Mode::Normal))
            .collect();
        self.quiz.marks = vec![Some(true); self.quiz.inputs.len()];
        self.quiz.status = Status::Revealed;
    }

    fn next_quiz(&mut self) {
        if self.quiz.index + 1 < self.current().quiz.len() {
            self.start_quiz(self.quiz.index + 1);
        } else {
            if self.score == self.current().quiz.len() {
                self.done[self.lesson] = true;
            }
            self.paused = None;
            self.goto(Screen::Result);
        }
    }

    /// 결과 화면: Enter 는 다음 레슨 보기 (마지막 레슨이면 목차 팝업), ^O 는 이 레슨 보기
    fn result_key(&mut self, code: KeyCode, ctrl: bool) -> bool {
        match code {
            KeyCode::Char('o') if ctrl => self.open_lesson(self.lesson),
            _ if ctrl => return false,
            KeyCode::Enter if self.lesson + 1 < self.lessons.len() => {
                self.open_lesson(self.lesson + 1);
            }
            KeyCode::Enter => self.open(Popup::Lessons),
            KeyCode::Char('q') => self.open(Popup::Quit),
            _ => return false,
        }
        true
    }

    /// 지금 편집 중인 편집기 (편집 중이 아니면 None)
    fn editor(&self) -> Option<&Editor> {
        match self.screen {
            Screen::Quiz => self.quiz.editor(),
            Screen::Lesson if self.editing => self.draft(),
            _ => None,
        }
    }

    fn editing_mode(&self) -> Option<Mode> {
        self.editor().map(Editor::mode)
    }

    pub fn cursor_style(&self) -> SetCursorStyle {
        match self.editing_mode() {
            Some(Mode::Insert) => SetCursorStyle::SteadyBar,
            Some(Mode::Normal) => SetCursorStyle::SteadyBlock,
            // 레슨 보기 커서는 vim Normal 처럼 블록
            None if self.screen == Screen::Lesson && self.popup.is_none() => {
                SetCursorStyle::SteadyBlock
            }
            None => SetCursorStyle::DefaultUserShape,
        }
    }
}

/// 편집기에 넘길 키 (Press 만, ^W 밖의 Ctrl/Alt 조합은 앱 몫이라 None)
fn vim_key(key: KeyEvent) -> Option<Key> {
    // Insert 의 ^W 만 편집기 몫 (단어 지우기)
    if key.kind == KeyEventKind::Press
        && key.modifiers == KeyModifiers::CONTROL
        && key.code == KeyCode::Char('w')
    {
        return Some(Key::WordBackspace);
    }
    if key.kind != KeyEventKind::Press || !key.modifiers.difference(KeyModifiers::SHIFT).is_empty()
    {
        return None;
    }
    Some(match key.code {
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Tab => Key::Tab,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        _ => return None,
    })
}

/// 길이 len 인 목록에서 한 칸 위(up)/아래로 (양 끝에서 멈춘다)
fn step(i: usize, up: bool, len: usize) -> usize {
    if up {
        i.saturating_sub(1)
    } else {
        (i + 1).min(len - 1)
    }
}
