//! 화면 그리기: 화면별 렌더 함수, 팝업, 머리줄·하단 줄, 아래 패널

use playrust::BLANK;
use playrust::highlight::{Kind, highlight, highlight_lines};
use playrust::vim::{Editor, Mode};
use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, ListState, Padding, Paragraph, Widget};

use super::style::{
    BLACK, BLUE, COMMENT, CURSORLINE, CYAN, GREEN, ORANGE, PURPLE, RED, SEARCH_BG, WRONG_BG,
    YELLOW, badge, float_block, kind_style, mode_color, panel_block, rich, styled, text_area,
    toc_list, toc_number,
};
use super::text::{
    char_width, expand_tabs, find_all, follow, lines_width, prefix_width, to_u16, truncate,
    wrap_cells, wrap_cursor, wrap_lines,
};
use super::{App, Output, Popup, Screen, Scroll, Status};

/// `?` 키 도움말 팝업 내용. 들여쓰기 없는 줄은 제목(굵게), 두 칸 공백 앞 단어는 이름(청록),
/// 두 칸 공백이 없는 들여쓴 줄은 참고(흐리게)로 그린다
pub(super) const KEYS: &str = "\
Screens
  Lesson    j/k gg G move · ^E/^Y scroll · PgUp/PgDn ^F/^B ^D/^U page
            / search · n/N next/prev · Esc clear · ,r run · ,e edit
            ,R reset to original · ^O quiz (resumes) · q quit
  Editing   vim keys in the Example box (Enter too) · ,r run (NORMAL)
            PgUp/PgDn output · ^F/^B ^D/^U cursor · ^O done editing
  Quiz      type into the blank (INSERT) · Esc → NORMAL · Enter check
            Tab/Shift+Tab next/prev blank · ,h hint · ,a show solution
            ↑/↓ scroll code (j/k in NORMAL) · PgUp/PgDn ^F/^B ^D/^U panel
            ,r run my answer · ^O lesson ↔ quiz · solved: Enter next
  Result    Enter next lesson (last one: lessons) · ^O lesson · q quit
  Anywhere  ^L lessons · ? help (not in INSERT) · ^C quit (twice: at once)
            leader , + key: not in INSERT (Esc first), other keys cancel
vim keys in quiz blanks and example editor (* example editor only)
  INSERT    type · Backspace/Delete ^W · ←/→ Home/End · Esc → NORMAL
            Enter new line* · Tab 4 spaces* · ↑/↓ prev/next line*
  move      h l ← → · 0 ^ $ · w b e · j k ↑ ↓* · Enter* · gg G* · / n N*
  insert    i a I A · o O*
  edit      x D C · s S cc · dd dw cw ciw diw · r<char> · u undo · yy p P*
  Unsupported: counts (3j), visual, : commands, regex, redo";

/// leader(`,`) 를 누른 뒤 하단 줄에 보이는 다음 키 안내
pub(super) const LEADER_HELP: &str = "r run · any other key cancels";
pub(super) const LESSON_LEADER_HELP: &str = "r run · e edit · R reset · any other key cancels";
pub(super) const QUIZ_LEADER_HELP: &str =
    "r run · h hint · a show solution · any other key cancels";
/// 맞히거나 정답을 본 뒤 (a 는 할 일이 없다)
pub(super) const LOCKED_LEADER_HELP: &str = "r run · h hint · any other key cancels";

pub(super) const QUIZ_TIP: &str =
    "Tip: vim keys in NORMAL (Esc): h l w b x cw ciw diw dd u · ? all keys";
/// 빈칸이 둘 이상이면 빈칸 이동부터 알린다 (첫 칸만 채우고 채점하지 않게)
pub(super) const BLANKS_TIP: &str =
    "Tip: Tab/Shift+Tab next/prev blank · vim keys in NORMAL (Esc) · ? all keys";

pub fn ui(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Lesson => lesson_ui(frame, app),
        Screen::Quiz => quiz_ui(frame, app),
        Screen::Result => result_ui(frame, app),
    }
    match app.popup {
        Some(Popup::Keys) => keys_ui(frame, app),
        Some(Popup::Quit) => quit_ui(frame),
        Some(Popup::Lessons) => lessons_ui(frame, app),
        None => {}
    }
}

/// 머리줄과 하단 줄을 그리고 가운데 영역을 돌려준다.
/// leader 를 누른 뒤에는 하단 단축키 줄 대신 다음 키 안내를 그린다
fn chrome(frame: &mut Frame, app: &App, header: Line, foot: Line) -> Rect {
    let [head, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(header, head);
    // vim 처럼 `/` 검색어를 입력하는 동안 하단 줄은 입력 줄
    if let Some(query) = &app.prompt {
        let prompt = Line::from(format!("/{query}"));
        let x = bottom.x.saturating_add(to_u16(prompt.width()));
        frame.render_widget(prompt, bottom);
        frame.set_cursor_position(Position::new(
            x.min(bottom.right().saturating_sub(1)),
            bottom.y,
        ));
        return body;
    }
    let foot = if app.leader {
        // lualine 의 command 모드 색 (보라)
        let keys = match app.screen {
            Screen::Quiz if app.quiz.status == Status::Answering => QUIZ_LEADER_HELP,
            Screen::Quiz => LOCKED_LEADER_HELP,
            Screen::Lesson if !app.editing => LESSON_LEADER_HELP,
            _ => LEADER_HELP,
        };
        status_line(badge(" LEADER ", PURPLE), keys)
    } else {
        foot
    };
    frame.render_widget(foot, bottom);
    body
}

fn help(text: &str) -> Line<'_> {
    Line::from(text).dim()
}

/// nvim lualine 처럼 색 배지 뒤에 흐린 안내
fn status_line<'a>(badge: Span<'a>, text: &'a str) -> Line<'a> {
    Line::from(vec![badge, " ".into(), text.dim()])
}

fn mode_line(mode: Mode, text: &str) -> Line<'_> {
    let name = match mode {
        Mode::Insert => " INSERT ",
        Mode::Normal => " NORMAL ",
    };
    status_line(badge(name, mode_color(mode)), text)
}

fn output_lines(output: &Output) -> Vec<Line<'_>> {
    match output {
        Output::Running(_) => vec!["Running…".bold().into()],
        Output::Done(run) => {
            let title = if run.success {
                "Run OK".fg(GREEN).bold()
            } else {
                "Run failed".fg(RED).bold()
            };
            let mut lines = vec![title.into()];
            if run.output.trim().is_empty() {
                lines.push("(no output)".dim().into());
            } else {
                // 제어 문자인 탭은 ratatui 가 버리므로 칸으로 펼친다
                lines.extend(
                    run.output
                        .trim_end()
                        .lines()
                        .map(|line| Line::from(expand_tabs(vec![Span::raw(line.to_string())]))),
                );
            }
            lines
        }
    }
}

/// 화면 아래 패널 (실행 출력, 퀴즈 힌트·해설). 본문 폭 전체를 쓰고 내용은 박스 안쪽 폭으로 접어 둔다.
/// 높이는 테두리 포함 내용 줄 수에 맞추되 max 까지
struct Panel<'a> {
    lines: Vec<Line<'static>>,
    block: Block<'a>,
    height: u16,
}

impl<'a> Panel<'a> {
    fn new(lines: &[Line], block: Block<'a>, body: Rect, max: u16) -> Self {
        let lines = wrap_lines(lines, block.inner(body).width);
        // 위아래 테두리 2줄
        let height = to_u16(lines.len()).saturating_add(2).min(max);
        Self {
            lines,
            block,
            height,
        }
    }
}

/// 아래 패널 자리의 높이 (패널이 없으면 팁 한 줄)
fn panel_height(panel: Option<&Panel>) -> u16 {
    panel.map_or(1, |p| p.height)
}

/// 아래 패널을 PgUp/PgDn 스크롤로 그린다. 패널이 없으면 그 자리에 팁 한 줄
fn render_panel(frame: &mut Frame, area: Rect, panel: Option<Panel>, tip: &str, scroll: &Scroll) {
    match panel {
        Some(Panel { lines, block, .. }) => {
            render_scrolled(frame, area, lines, block, scroll, " PgUp/PgDn ");
        }
        None => frame.render_widget(help(tip), area),
    }
}

/// 접어 둔 줄들을 block 안에 scroll 위치부터 그린다. 위치는 내용 끝을 넘지 않게 보정해
/// scroll 에 저장하고 돌려준다. 내용이 넘치면 테두리 오른쪽 위에 스크롤 키(hint)를 표시한다
fn render_scrolled(
    frame: &mut Frame,
    area: Rect,
    lines: Vec<Line>,
    block: Block,
    scroll: &Scroll,
    hint: &str,
) -> u16 {
    let rows = to_u16(lines.len());
    let inner_h = block.inner(area).height;
    scroll.height.set(inner_h);
    let offset = scroll.get().min(rows.saturating_sub(inner_h));
    scroll.set(offset);
    let block = if rows > inner_h {
        block.title_top(Line::from(hint).right_aligned())
    } else {
        block
    };
    frame.render_widget(Paragraph::new(lines).block(block).scroll((offset, 0)), area);
    offset
}

/// 화면 가운데에 안쪽 크기 width x height 인 팝업 자리를 비우고 테두리 포함 영역을 돌려준다
/// (화면보다 크면 화면에 맞춘다)
fn popup_area(frame: &mut Frame, width: u16, height: u16) -> Rect {
    let full = frame.area();
    let area = full.centered(
        Constraint::Length(width.saturating_add(4)),
        Constraint::Length(height.saturating_add(2)),
    );
    // 왼쪽 경계에 걸친 2칸 문자가 테두리를 가리지 않게 좌우 1칸도 비운다
    frame.render_widget(Clear, area.outer(Margin::new(1, 0)).intersection(full));
    area
}

pub(super) fn keys_lines() -> Vec<Line<'static>> {
    KEYS.lines()
        .map(|line| match line.strip_prefix("  ") {
            None => line.bold().into(),
            Some(rest) => match rest.find("  ") {
                Some(0) => line.into(),
                Some(i) => Line::from(vec!["  ".into(), rest[..i].fg(CYAN), rest[i..].into()]),
                None => line.dim().into(),
            },
        })
        .collect()
}

/// 화면 가운데 키 도움말 팝업 (넘치면 ↑/↓ 스크롤)
fn keys_ui(frame: &mut Frame, app: &App) {
    let keys = keys_lines();
    let width = lines_width(&keys).min(frame.area().width.saturating_sub(4));
    let lines = wrap_lines(&keys, width);
    let area = popup_area(frame, width, to_u16(lines.len()));
    let block = float_block(" Keys ").title_bottom(Line::from(" ?/Esc close ").right_aligned());
    render_scrolled(frame, area, lines, block, &app.keys_scroll, " ↑/↓ ");
}

fn quit_ui(frame: &mut Frame) {
    let lines = vec![
        Line::from("Quit playrust?").bold(),
        help("y/Enter quit · n/Esc cancel"),
    ];
    let area = popup_area(frame, lines_width(&lines), to_u16(lines.len()));
    frame.render_widget(
        Paragraph::new(lines)
            .centered()
            .block(float_block(" Quit ")),
        area,
    );
}

/// 목차 팝업: 제목에 완료 수, 줄마다 번호·제목·✓·요약(회색). 현재 레슨은 파랑 굵게, 고른 레슨은
/// 초록 바탕. 화면보다 넓은 줄은 끝을 … 로 줄이고, 낮은 화면에서는 고른 레슨을 따라 스크롤
fn lessons_ui(frame: &mut Frame, app: &App) {
    let rows: Vec<Line> = app
        .lessons
        .iter()
        .zip(&app.done)
        .enumerate()
        .map(|(i, (lesson, done))| {
            let mut spans = vec![toc_number(i, i == app.pick)];
            spans.push(if i == app.lesson {
                lesson.title.as_str().fg(BLUE).bold()
            } else {
                lesson.title.as_str().into()
            });
            if *done {
                spans.push(" ✓".fg(GREEN));
            }
            spans.extend(["  ".into(), lesson.summary.as_str().fg(COMMENT)]);
            Line::from(spans)
        })
        .collect();
    // 선택 기호 "> " 2칸
    let width = lines_width(&rows)
        .saturating_add(2)
        .min(frame.area().width.saturating_sub(4));
    let items: Vec<Line> = rows
        .into_iter()
        .map(|row| truncate(row, width.saturating_sub(2)))
        .collect();
    let done = app.done.iter().filter(|d| **d).count();
    let title = format!(" Lessons {done}/{} ", app.lessons.len());
    let area = popup_area(frame, width, to_u16(items.len()));
    let list = toc_list(items).block(
        float_block(&title).title_bottom(Line::from(" Enter open · Esc close ").right_aligned()),
    );
    let mut state = ListState::default().with_selected(Some(app.pick));
    frame.render_stateful_widget(list, area, &mut state);
}

/// 레슨 제목 (수정본이 있으면 표시)
fn lesson_title(app: &App) -> Vec<Span<'_>> {
    let mut spans = vec![format!("{}. {}", app.lesson + 1, app.current().title).bold()];
    if app.modified() {
        spans.push(" (modified)".fg(YELLOW));
    }
    spans
}

/// 레슨 화면. 예제 편집도 화면을 바꾸지 않고 이 화면의 ` Example ` 박스 안에서 한다: 편집 중에는
/// 박스 테두리가 모드 색이 되고 박스 안에 커서가 생길 뿐, 설명·박스·출력 패널 자리와 코드 접기는
/// 보기와 같아 편집을 시작하거나 마쳐도 화면이 움직이지 않는다
fn lesson_ui(frame: &mut Frame, app: &App) {
    let lesson = app.current();
    let editor = app.editor();
    let mut header = lesson_title(app);
    header.extend(["  ".into(), lesson.summary.as_str().dim()]);
    let foot = match editor.map(Editor::mode) {
        Some(Mode::Insert) => mode_line(
            Mode::Insert,
            "Esc, then ,r run · ^O done editing · ^L lessons",
        ),
        Some(Mode::Normal) => mode_line(
            Mode::Normal,
            "i/a/o insert · ,r run · ^O done editing · ^L lessons · ? help",
        ),
        None => {
            let keys = if app.modified() {
                ",r run · ,e edit · ,R reset"
            } else {
                "j/k move · ,r run · ,e edit"
            };
            // 퀴즈를 풀다 왔으면 ^O 로 돌아갈 수 있다고 알린다
            let quiz = if app.paused == Some(app.lesson) {
                "^O back to quiz"
            } else {
                "^O quiz"
            };
            Line::from(format!("{keys} · {quiz} · ^L lessons · ? help · q quit")).dim()
        }
    };
    let body = chrome(frame, app, Line::from(header), foot);

    let output = app.output.as_ref().map(|o| {
        // 위 예제 박스와 글 시작 칸을 맞춘다
        let block = panel_block()
            .title(" Output ")
            .padding(Padding::horizontal(1));
        Panel::new(&output_lines(o), block, body, body.height * 2 / 5)
    });
    let output_h = output.as_ref().map_or(0, |o| o.height);
    let [main, panel] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(output_h)]).areas(body);
    // 설명, 빈 줄, 예제 박스를 한 장(page)에 다 그린 뒤 보이는 줄만 옮긴다.
    // 같이 스크롤되면서도 화면 밖으로 반쯤 나간 박스의 테두리가 온전하게 남는다.
    // 박스는 아래 실행 패널과 폭을 맞추고, 코드는 nvim 의 `wrap` 처럼 글자 단위로 접는다
    // (편집 중에도 접은 채로 그리고 커서를 접힌 자리에 둔다)
    let text_w = text_area(main).width;
    let concept = wrap_lines(&rich(&lesson.concept), text_w);
    let concept_h = to_u16(concept.len());
    let code_top = concept_h.saturating_add(1);
    let source: Vec<&str> = match app.draft() {
        Some(draft) => draft.lines().iter().map(String::as_str).collect(),
        None => lesson.example.trim_end().lines().collect(),
    };
    let mut block = float_block(" Example ");
    let inner_w = usize::from(block.inner(main).width);
    // 원본 줄마다 접힌 첫 줄이 박스 안 몇 번째 줄인지
    let mut starts = Vec::with_capacity(source.len());
    let mut code: Vec<Line> = Vec::new();
    for tokens in highlight_lines(source.iter().copied()) {
        starts.push(code.len());
        code.extend(wrap_cells(expand_tabs(styled(tokens)), inner_w));
    }
    let code_rows = code.len();
    let code_h = to_u16(code_rows).saturating_add(2);
    let total = code_top.saturating_add(code_h);
    // 편집 중이면 커서의 (박스 안 줄, 페이지 줄, 박스 안쪽 칸)
    let cursor = editor.map(|editor| {
        let (row, at) = editor.cursor();
        let (wrap, x) = wrap_cursor(source[row], at, inner_w);
        let k = starts[row] + wrap;
        (k, code_top.saturating_add(1).saturating_add(to_u16(k)), x)
    });

    let max = total.saturating_sub(main.height);
    app.scroll.height.set(main.height);
    app.scroll.max.set(max);
    let mut offset = app.scroll.get().min(max);
    // 편집 중에는 커서 줄이 늘 보이게 페이지를 옮긴다. 화면 밖 첫 줄·끝 줄로 가면 박스 테두리까지
    if let Some((k, y, _)) = cursor
        && !(offset..offset.saturating_add(main.height)).contains(&y)
    {
        if k + 1 == code.len() {
            offset = follow(offset, y.saturating_add(1), main.height);
        }
        if k == 0 {
            offset = follow(offset, y - 1, main.height);
        }
        offset = follow(offset, y, main.height).min(max);
    }
    match cursor {
        // 편집을 마치면 보기 커서가 편집기 커서 줄에 남는다
        Some((_, y, _)) => app.cursor_row.set(y),
        // 보기 커서는 내용 끝으로 자르고, 화면 밖이면 vim 처럼 화면이 커서를 따라간다
        // (출력 패널이 생기거나 터미널이 줄어도 커서 줄은 그대로)
        None => {
            let row = app.cursor_row.get().min(total.saturating_sub(1));
            app.cursor_row.set(row);
            offset = follow(offset, row, main.height).min(max);
        }
    }

    if let Some(editor) = editor {
        let (row, at) = editor.cursor();
        // 테두리는 하단 배지와 같은 모드 색, 오른쪽 위에 커서 위치 (라벨은 초록 그대로)
        block = block
            .border_style(Style::new().fg(mode_color(editor.mode())))
            .title_top(Line::from(format!(" {}:{} ", row + 1, at + 1)).right_aligned());
    }
    let code_area = Rect::new(0, code_top, main.width, code_h);
    let inner = block.inner(code_area);
    // 편집을 마치면 보기 커서가 편집기 커서 칸에 남는다
    if let Some((_, _, x)) = cursor {
        app.cursor_x.set(Some(inner.x.saturating_add(to_u16(x))));
    }
    let mut page = Buffer::empty(Rect::new(0, 0, main.width, total));
    Paragraph::new(concept).render(Rect::new(0, 0, text_w, concept_h), &mut page);
    Paragraph::new(code)
        .block(block)
        .render(code_area, &mut page);
    // 검색어(입력 중이면 입력 중인 것)를 설명과 예제에서 보이는 그대로 찾는다
    let query = app.prompt.as_deref().filter(|q| !q.is_empty());
    let mut hits = Vec::new();
    if let Some(query) = query.or(app.search.as_deref()) {
        let query: Vec<char> = query.chars().collect();
        hits = find_cells(&page, Rect::new(0, 0, text_w, concept_h), &query);
        hits.extend(find_cells(&page, inner, &query));
    }
    if let Some((forward, inclusive)) = app.jump.take().filter(|_| editor.is_none()) {
        match jump(app, &hits, offset, main.height, forward, inclusive) {
            Some(to) => offset = to.min(max),
            None => app.not_found.set(true),
        }
    }
    // 보기 커서 줄은 nvim 의 CursorLine 처럼 바탕을 칠하고, 커서가 놓인 일치는 지금 일치로 칠한다
    let cursor_row = editor.is_none().then(|| app.cursor_row.get());
    let cursor_x = app.cursor_x.get().filter(|_| editor.is_none());
    if let Some(row) = cursor_row {
        for x in 0..main.width {
            page[(x, row)].set_bg(CURSORLINE);
        }
    }
    for &(y, from, to) in &hits {
        let style = if cursor_row == Some(y) && cursor_x == Some(from) {
            Style::new().fg(BLACK).bg(ORANGE).bold()
        } else {
            Style::new().bg(SEARCH_BG)
        };
        for x in from..to {
            page[(x, y)].set_style(style);
        }
    }
    // 보기 커서가 예제 위면 편집을 시작할 (줄, 글자). 검색으로 간 칸이 있으면 그 글자,
    // 수정본 커서가 같은 접힌 줄에 있으면 그 자리, 아니면 접힌 줄의 첫 글자 (커서 줄이 그대로다)
    let code_first = code_top.saturating_add(1);
    app.cursor_line.set(cursor_row.and_then(|row| {
        let k = usize::from(row.checked_sub(code_first)?);
        (k < code_rows).then(|| {
            let line = starts.partition_point(|&s| s <= k) - 1;
            let text = source[line];
            let wrap = k - starts[line];
            let at = |c: usize| wrap_cursor(text, c, inner_w);
            let x = cursor_x.map(|x| usize::from(x.saturating_sub(inner.x)));
            let kept = app
                .draft()
                .map(Editor::cursor)
                .filter(|&(r, c)| r == line && at(c).0 == wrap)
                .map(|(_, c)| c);
            let cols = 0..=text.chars().count();
            let col = x
                .and_then(|x| cols.clone().find(|&c| at(c) == (wrap, x)))
                .or(kept)
                .or_else(|| cols.clone().find(|&c| at(c).0 == wrap))
                .unwrap_or(0);
            (line, col)
        })
    }));
    app.scroll.set(offset);
    // 첫 줄이 보이는 예제 줄 범위 (편집을 시작할 때 커서를 이 안으로 옮긴다)
    let first = usize::from(code_top) + 1;
    let lo = usize::from(offset).saturating_sub(first);
    let hi = (usize::from(offset) + usize::from(main.height)).saturating_sub(first);
    app.view_lines.set((
        starts.partition_point(|&s| s < lo),
        starts.partition_point(|&s| s < hi),
    ));
    let buf = frame.buffer_mut();
    for y in 0..main.height.min(total - offset) {
        for x in 0..main.width {
            buf[(main.x + x, main.y + y)] = page[(x, offset + y)].clone();
        }
    }
    // 꽉 찬 줄 끝 뒤(Insert)의 커서는 박스 안쪽 오른쪽 여백 칸에 둔다
    if let Some((_, y, x)) = cursor
        && app.popup.is_none()
        && app.prompt.is_none()
        && inner.width > 0
        && (offset..offset.saturating_add(main.height)).contains(&y)
    {
        let x = to_u16(x).min(inner.width);
        frame.set_cursor_position(Position::new(main.x + inner.x + x, main.y + y - offset));
    }
    // 보기 커서는 검색으로 간 일치 칸, 아니면 줄 첫 칸 (예제 박스 안이면 박스 안쪽 첫 칸)
    if let Some(row) = cursor_row
        && app.popup.is_none()
        && app.prompt.is_none()
        && (offset..offset.saturating_add(main.height)).contains(&row)
    {
        let in_box = (code_first..code_first.saturating_add(to_u16(code_rows))).contains(&row);
        let x = cursor_x.unwrap_or(if in_box { inner.x } else { 0 });
        frame.set_cursor_position(Position::new(main.x + x, main.y + row - offset));
    }

    // 출력이 넘치면 테두리에 스크롤 키를 알린다 (보기에서는 PgUp/PgDn 이 본문이라 편집에 들어가서)
    if let Some(Panel { lines, block, .. }) = output {
        let hint = match editor {
            Some(_) => " PgUp/PgDn ",
            None => " ,e then PgUp/PgDn ",
        };
        render_scrolled(frame, panel, lines, block, &app.panel_scroll, hint);
    }
    // 검색 이동에서 일치가 없으면 vim 처럼 하단 줄에 알린다 (다음 키까지)
    if app.not_found.get() {
        let area = frame.area();
        let bottom = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
        let query = app.search.as_deref().unwrap_or_default();
        frame.render_widget(Clear, bottom);
        frame.render_widget(
            Line::from(format!("Pattern not found: {query}")).fg(RED),
            bottom,
        );
    }
}

/// page 의 area 안에서 query 와 같은 글자가 이어진 칸 (줄, 시작 x, 끝 x). 화면에 보이는 그대로
/// 줄마다 찾으므로 접힌 자리를 넘는 일치는 찾지 않는다. 두 칸 글자 뒤의 빈 칸은 건너뛰고,
/// 줄 끝 뒤 빈 칸은 글자가 아니므로 버린다 (공백이 든 검색어가 줄 끝과 일치하지 않게)
fn find_cells(page: &Buffer, area: Rect, query: &[char]) -> Vec<(u16, u16, u16)> {
    let mut hits = Vec::new();
    for y in area.top()..area.bottom() {
        let mut cells = Vec::new();
        let mut x = area.left();
        while x < area.right() {
            let c = page[(x, y)].symbol().chars().next().unwrap_or(' ');
            let w = to_u16(char_width(c).max(1));
            cells.push((x, w, c));
            x = x.saturating_add(w);
        }
        while cells.last().is_some_and(|&(_, _, c)| c == ' ') {
            cells.pop();
        }
        let chars: Vec<char> = cells.iter().map(|&(.., c)| c).collect();
        for i in find_all(&chars, query) {
            let (last, w, _) = cells[i + query.len() - 1];
            hits.push((y, cells[i].0, last.saturating_add(w).min(area.right())));
        }
    }
    hits
}

/// 레슨 보기의 검색 이동: 보기 커서를 커서 다음(이전) 일치로 옮기고 (inclusive 면 커서 자리부터,
/// 끝에서 처음으로 돈다), 그 줄이 화면 밖이면 위에서 1/3 자리에 오게 스크롤한 offset 을 돌려준다.
/// 일치가 없으면 None
fn jump(
    app: &App,
    hits: &[(u16, u16, u16)],
    offset: u16,
    height: u16,
    forward: bool,
    inclusive: bool,
) -> Option<u16> {
    let base = (app.cursor_row.get(), app.cursor_x.get().unwrap_or(0));
    let at = |&&(y, x, _): &&(u16, u16, u16)| (y, x);
    let target = if forward {
        hits.iter()
            .find(|h| at(h) > base || (inclusive && at(h) == base))
            .or(hits.first())
    } else {
        hits.iter().rev().find(|h| at(h) < base).or(hits.last())
    };
    let &(row, x, _) = target?;
    app.cursor_row.set(row);
    app.cursor_x.set(Some(x));
    Some(if (offset..offset.saturating_add(height)).contains(&row) {
        offset
    } else {
        row.saturating_sub(height / 3)
    })
}

/// 빈칸 자리에 입력값을 끼운 코드 줄과, 현재 빈칸 편집기 커서의 (행, 열)
fn code_lines(app: &App) -> (Vec<Line<'static>>, (u16, u16)) {
    let quiz = &app.quiz;
    let mut blank = 0;
    let mut cursor = (0, 0);
    let mut lines = Vec::new();
    let code = highlight(app.current_quiz().code.trim_end());
    for (row, tokens) in code.into_iter().enumerate() {
        let mut spans: Vec<Span> = Vec::new();
        for (kind, text) in tokens {
            if kind != Kind::Blank {
                spans.push(Span::styled(text, kind_style(kind)));
                continue;
            }
            let editor = quiz.inputs.get(blank);
            let input = editor.map(Editor::text).unwrap_or_default();
            // 채점한 빈칸은 바탕까지 칠해 같은 색 글자(빨강 변수, 초록 문자열)와 구분한다
            let mut style = match quiz.marks.get(blank).copied().flatten() {
                Some(true) => Style::new().fg(GREEN).bg(CURSORLINE).bold(),
                Some(false) => Style::new().fg(RED).bg(WRONG_BG).bold(),
                None => Style::new(),
            }
            .underlined();
            if blank == quiz.blank && quiz.status == Status::Answering {
                style = style.reversed();
                let before: usize = spans.iter().map(Span::width).sum();
                let at = editor.map_or(0, |e| e.cursor().1);
                cursor = (to_u16(row), to_u16(before + prefix_width(&input, at)));
            }
            spans.push(if input.is_empty() {
                Span::styled(BLANK, style.dim())
            } else {
                Span::styled(input, style)
            });
            blank += 1;
        }
        lines.push(Line::from(spans));
    }
    (lines, cursor)
}

fn quiz_panel(app: &App) -> Vec<Line<'_>> {
    let quiz = app.current_quiz();
    let last = app.quiz.index + 1 == app.current().quiz.len();
    let next = if last { "results" } else { "next quiz" };
    let mut lines: Vec<Line> = Vec::new();
    match app.quiz.status {
        Status::Answering if app.quiz.marks.contains(&Some(false)) => {
            lines.push(
                "✗ Some blanks are wrong (red). Fix them and press Enter again"
                    .fg(RED)
                    .into(),
            );
        }
        Status::Answering => {}
        Status::Solved => lines.push(format!("Correct! Enter: {next}").fg(GREEN).bold().into()),
        Status::Revealed => lines.push(
            format!("Answers filled in (not scored). Enter: {next}")
                .bold()
                .into(),
        ),
    }
    if app.quiz.hint {
        let mut hint = vec!["Hint: ".fg(PURPLE).bold()];
        hint.extend(rich(&quiz.hint).into_iter().flat_map(|l| l.spans));
        lines.push(Line::from(hint));
    }
    if app.quiz.status != Status::Answering {
        let mut explanation = rich(&quiz.explanation);
        if let Some(first) = explanation.first_mut() {
            first.spans.insert(0, "Explanation: ".bold());
        }
        lines.extend(explanation);
    }
    if let Some(output) = &app.output {
        if !lines.is_empty() {
            lines.push(Line::default());
        }
        lines.extend(output_lines(output));
    }
    lines
}

fn quiz_ui(frame: &mut Frame, app: &App) {
    let lesson = app.current();
    let header = Line::from(vec![
        format!("Quiz {}/{}", app.quiz.index + 1, lesson.quiz.len()).bold(),
        format!("  {}. {}", app.lesson + 1, lesson.title).dim(),
    ]);
    let foot = match app.editing_mode() {
        Some(Mode::Insert) => mode_line(
            Mode::Insert,
            "Enter check · Esc, then ,h hint / ,a solution · ^O lesson · ^L lessons",
        ),
        Some(Mode::Normal) => mode_line(
            Mode::Normal,
            "Enter check · ,h hint · ,a solution · ^O lesson · ^L lessons · ? help",
        ),
        None => help("Enter next · ,h hint · ,r run · ^O lesson · ^L lessons · ? help"),
    };
    let body = chrome(frame, app, header, foot);

    let prompt = wrap_lines(&rich(&app.current_quiz().prompt), text_area(body).width);
    let prompt_h = to_u16(prompt.len()).min(3);

    let panel_lines = quiz_panel(app);
    let panel = (!panel_lines.is_empty())
        .then(|| Panel::new(&panel_lines, panel_block(), body, body.height / 2));

    let [prompt_area, code_area, panel_area] = Layout::vertical([
        Constraint::Length(prompt_h),
        Constraint::Min(0),
        Constraint::Length(panel_height(panel.as_ref())),
    ])
    .areas(body);
    frame.render_widget(Paragraph::new(prompt), text_area(prompt_area));

    let (lines, (row, col)) = code_lines(app);
    // 빈칸을 입력하는 동안은 테두리를 하단 배지와 같은 모드 색으로
    let block = Block::bordered().title(" Rust ");
    let block = match app.editing_mode() {
        Some(mode) => block.border_style(Style::new().fg(mode_color(mode))),
        None => block,
    };
    let inner = block.inner(code_area);
    if app.quiz.follow {
        app.quiz_scroll
            .set(follow(app.quiz_scroll.get(), row, inner.height));
    }
    let offset = render_scrolled(frame, code_area, lines, block, &app.quiz_scroll, " ↑/↓ ");
    if app.quiz.status == Status::Answering
        && app.popup.is_none()
        && (offset..offset.saturating_add(inner.height)).contains(&row)
        && col < inner.width
    {
        frame.set_cursor_position(Position::new(inner.x + col, inner.y + row - offset));
    }

    let tip = if app.quiz.inputs.len() > 1 {
        BLANKS_TIP
    } else {
        QUIZ_TIP
    };
    render_panel(frame, panel_area, panel, tip, &app.panel_scroll);
}

fn result_ui(frame: &mut Frame, app: &App) {
    let lesson = app.current();
    let total = lesson.quiz.len();
    let next = app.lessons.get(app.lesson + 1);
    let keys = if next.is_some() {
        "Enter next lesson · ^O lesson · ^L lessons · ? help · q quit"
    } else {
        "Enter lessons · ^O lesson · ^L lessons · ? help · q quit"
    };
    let body = chrome(
        frame,
        app,
        format!("{}. {} - Result", app.lesson + 1, lesson.title)
            .bold()
            .into(),
        help(keys),
    );
    let mut lines: Vec<Line> = vec![
        Line::default(),
        format!("Score: {}/{total}", app.score).bold().into(),
        Line::default(),
    ];
    if app.score == total {
        lines.push("All quizzes correct! ✓".fg(GREEN).bold().into());
    } else {
        lines.push(
            "Quizzes answered with ,a (show solution) are not scored."
                .dim()
                .into(),
        );
    }
    lines.push(Line::default());
    lines.push(match next {
        Some(next) => format!("Enter: next lesson {}. {}", app.lesson + 2, next.title).into(),
        None => "You finished the last lesson! Enter: lessons".into(),
    });
    let area = text_area(body);
    frame.render_widget(
        Paragraph::new(wrap_lines(&lines, area.width)).centered(),
        area,
    );
}
