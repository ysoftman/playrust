//! 색 팔레트와 구문 강조 스타일, 떠 있는 창·목차 모양

use playrust::highlight::{Kind, highlight};
use playrust::vim::Mode;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, List, Padding};

// 사용자 nvim 테마 onedarkpro `onedark_dark` 의 truecolor 팔레트 (배경은 터미널 기본값을 그대로 쓴다)
pub(super) const RED: Color = Color::Rgb(0xef, 0x59, 0x6f);
pub(super) const ORANGE: Color = Color::Rgb(0xd1, 0x9a, 0x66);
pub(super) const YELLOW: Color = Color::Rgb(0xe5, 0xc0, 0x7b);
pub(super) const GREEN: Color = Color::Rgb(0x89, 0xca, 0x78);
pub(super) const CYAN: Color = Color::Rgb(0x2b, 0xba, 0xc5);
pub(super) const BLUE: Color = Color::Rgb(0x61, 0xaf, 0xef);
pub(super) const PURPLE: Color = Color::Rgb(0xd5, 0x5f, 0xde);
pub(super) const COMMENT: Color = Color::Rgb(0x7f, 0x84, 0x8e);
pub(super) const GRAY: Color = Color::Rgb(0x43, 0x48, 0x52);
/// onedark_dark 의 배경색. lualine 모드 배지와 지금 검색 일치의 글자색으로 쓴다
pub(super) const BLACK: Color = Color::Rgb(0x00, 0x00, 0x00);
/// 사용자가 nvim 에 지정한 CursorLine 바탕 (목차 선택 줄, 맞힌 빈칸, 레슨 보기 커서 줄)
pub(super) const CURSORLINE: Color = Color::Rgb(0x00, 0x3f, 0x00);
/// 검색 일치 바탕. 지금 일치(주황 바탕, 검은 글자)와 구분되게 어둡게 깔고 글자색은 그대로 둔다
pub(super) const SEARCH_BG: Color = Color::Rgb(0x4b, 0x41, 0x1e);
/// CURSORLINE 의 빨강 짝 (틀린 빈칸)
pub(super) const WRONG_BG: Color = Color::Rgb(0x3f, 0x00, 0x00);

/// 사용자 nvim(onedarkpro `onedark_dark` + nvim-treesitter + rust-analyzer)에서 Rust 코드가 보이는 색.
/// rust-analyzer 가 매크로 이름은 상수색(주황), 포맷 자리표시자는 문자열색으로 칠하는 것도 따른다.
/// 글자색만 쓰고 굵게·이탤릭은 쓰지 않는다. 변수가 빨강이라 퀴즈 오답은 빨간 바탕 칸으로 구분한다
pub(super) fn kind_style(kind: Kind) -> Style {
    let fg = match kind {
        Kind::Plain | Kind::Blank | Kind::Operator => return Style::new(),
        Kind::Keyword | Kind::Lifetime | Kind::Attribute | Kind::CallParen => PURPLE,
        Kind::Type | Kind::Module | Kind::SelfValue => YELLOW,
        Kind::Function => BLUE,
        Kind::Macro | Kind::Constant | Kind::Number | Kind::Bracket => ORANGE,
        Kind::Str | Kind::Char | Kind::Interpolation => GREEN,
        Kind::Escape | Kind::MacroParen | Kind::Variant | Kind::SpecialOperator => CYAN,
        Kind::Comment => COMMENT,
        Kind::Variable => RED,
    };
    Style::new().fg(fg)
}

/// nvim lualine(onedarkpro `onedark_dark` 테마) 의 모드 색: Normal 초록, Insert 파랑.
/// 하단 모드 배지와 편집 중인 코드 박스 테두리에 쓴다
pub(super) fn mode_color(mode: Mode) -> Color {
    match mode {
        Mode::Normal => GREEN,
        Mode::Insert => BLUE,
    }
}

/// lualine 의 모드 칸(a 섹션)처럼 색 바탕에 검은 굵은 글자 (` NORMAL `)
pub(super) fn badge(text: &str, bg: Color) -> Span<'_> {
    Span::styled(text, Style::new().fg(BLACK).bg(bg).bold())
}

/// 하이라이트한 코드 한 줄의 span
pub(super) fn styled(tokens: Vec<(Kind, String)>) -> Vec<Span<'static>> {
    tokens
        .into_iter()
        .map(|(kind, text)| Span::styled(text, kind_style(kind)))
        .collect()
}

/// 설명문의 인라인 `코드` 를 구문 강조. 색 없는 토큰(Plain, 연산자)은 nvim 마크다운 인라인 코드
/// (`@markup.raw`)처럼 초록으로 칠해 본문과 구분한다
pub(super) fn rich(text: &str) -> Vec<Line<'_>> {
    text.trim()
        .lines()
        .map(|line| {
            let spans: Vec<Span> = line
                .split('`')
                .enumerate()
                .flat_map(|(i, part)| {
                    if i % 2 == 0 {
                        return vec![Span::raw(part)];
                    }
                    highlight(part)
                        .into_iter()
                        .flatten()
                        .map(|(kind, text)| {
                            Span::styled(text, Style::new().fg(GREEN).patch(kind_style(kind)))
                        })
                        .collect()
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// nvim 떠 있는 창처럼 테두리는 회색(FloatBorder), 제목은 초록(Title), 나머지 테두리 글자는 흐리게.
/// 팝업과 예제 코드 박스가 함께 쓴다 (안쪽 좌우 1칸 여백)
pub(super) fn float_block(title: &str) -> Block<'_> {
    Block::bordered()
        .border_style(Style::new().fg(GRAY))
        .title_style(Style::new().fg(COMMENT))
        .title(Line::from(title).fg(GREEN).bold())
        .padding(Padding::horizontal(1))
}

/// 본문 글(설명·퀴즈 문제·결과)과 아래 패널 글의 오른쪽 여백 칸 수. 글이 화면 끝이나 패널 테두리에
/// 바로 붙지 않게 두는 디자인 여백이다 (2칸 문자가 넘치지 않게 접는 일은 `wrap_lines` 가 한다)
pub(super) const TEXT_MARGIN: u16 = 1;

/// 본문 글을 그리는 영역 (오른쪽 TEXT_MARGIN 칸을 비운다)
pub(super) fn text_area(area: Rect) -> Rect {
    Rect {
        width: area.width.saturating_sub(TEXT_MARGIN),
        ..area
    }
}

/// 화면 아래 패널(실행 출력, 퀴즈 힌트·해설) 박스 (안쪽 오른쪽 TEXT_MARGIN 칸 여백)
pub(super) fn panel_block() -> Block<'static> {
    Block::bordered().padding(Padding::right(TEXT_MARGIN))
}

/// 목차 번호 열: nvim 줄 번호처럼 고른 줄은 보라(CursorLineNr), 나머지는 흐리게
pub(super) fn toc_number(i: usize, selected: bool) -> Span<'static> {
    let style = if selected {
        Style::new().fg(PURPLE).bold()
    } else {
        Style::new().fg(COMMENT)
    };
    Span::styled(format!("{:>2}. ", i + 1), style)
}

/// 목차 팝업의 목록: 고른 줄은 nvim CursorLine 처럼 진한 초록 바탕
/// (글자색은 그대로 두려고 바탕만 지정)
pub(super) fn toc_list<'a>(items: Vec<Line<'a>>) -> List<'a> {
    List::new(items)
        .highlight_symbol(Line::from("> ").fg(GREEN).bold())
        .highlight_style(Style::new().bg(CURSORLINE))
}

#[cfg(test)]
mod tests {
    use playrust::highlight::Kind;
    use ratatui::style::Style;
    use ratatui::text::Line;

    use super::*;

    #[test]
    fn kind_style_uses_nvim_palette() {
        let fg = |kind| kind_style(kind).fg;
        for (kinds, color) in [
            (
                &[
                    Kind::Keyword,
                    Kind::Lifetime,
                    Kind::Attribute,
                    Kind::CallParen,
                ][..],
                PURPLE,
            ),
            (&[Kind::Type, Kind::Module, Kind::SelfValue], YELLOW),
            (&[Kind::Function], BLUE),
            (
                &[Kind::Macro, Kind::Constant, Kind::Number, Kind::Bracket],
                ORANGE,
            ),
            (&[Kind::Str, Kind::Char, Kind::Interpolation], GREEN),
            (
                &[
                    Kind::Escape,
                    Kind::MacroParen,
                    Kind::Variant,
                    Kind::SpecialOperator,
                ],
                CYAN,
            ),
            (&[Kind::Comment], COMMENT),
            (&[Kind::Variable], RED),
        ] {
            for &kind in kinds {
                assert_eq!(fg(kind), Some(color), "{kind:?}");
                assert_eq!(kind_style(kind), Style::new().fg(color), "{kind:?}");
            }
        }
        for kind in [Kind::Plain, Kind::Operator, Kind::Blank] {
            assert_eq!(kind_style(kind), Style::new(), "{kind:?}");
        }
    }

    /// line 에서 pat 이 시작하는 글자의 스타일
    fn style_at(line: &Line, pat: &str) -> Style {
        let text = line.to_string();
        let mut at = text
            .find(pat)
            .unwrap_or_else(|| panic!("{text:?} 에 {pat:?} 없음"));
        for span in &line.spans {
            if at < span.content.len() {
                return span.style;
            }
            at -= span.content.len();
        }
        unreachable!()
    }

    #[test]
    fn inline_code_in_text_is_highlighted() {
        let lines = rich("`let` 은 `\"a\\n{x}\"` 와 `x?` 그리고 짝 없는 `fn");
        let [line] = lines.as_slice() else {
            panic!("한 줄이어야 함")
        };
        assert_eq!(
            line.to_string(),
            "let 은 \"a\\n{x}\" 와 x? 그리고 짝 없는 fn"
        );
        assert_eq!(style_at(line, "let").fg, Some(PURPLE));
        assert_eq!(style_at(line, " 은").fg, None);
        assert_eq!(style_at(line, "\"a").fg, Some(GREEN));
        assert_eq!(style_at(line, "\\n").fg, Some(CYAN));
        // 값 뒤의 ? 는 코드와 같은 청록, 굵게 없음
        let op = style_at(line, "? ");
        assert_eq!(op.fg, Some(CYAN));
        assert!(op.add_modifier.is_empty());
        assert_eq!(style_at(line, "fn").fg, Some(PURPLE));
        // 색 없는 토큰(일반 연산자 등)은 nvim 마크다운 인라인 코드처럼 초록으로 본문과 구분
        let plus = style_at(&rich("`1 + 2`")[0], "+");
        assert_eq!(plus.fg, Some(GREEN));
        assert!(plus.add_modifier.is_empty());
        assert_eq!(rich("백틱 없음")[0].spans[0].style, Style::new());
    }
}
