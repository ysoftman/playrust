//! 2칸 문자(한글)를 고려한 폭·스크롤·줄바꿈 계산

use std::collections::VecDeque;

use ratatui::buffer::CellWidth;
use ratatui::style::Style;
use ratatui::text::{Line, Span, StyledGrapheme};

/// pos 가 보이도록 [offset, offset + len) 창을 옮긴다
pub(super) fn follow(offset: u16, pos: u16, len: u16) -> u16 {
    if pos < offset {
        pos
    } else if len > 0 && pos >= offset.saturating_add(len) {
        pos.saturating_add(1) - len
    } else {
        offset
    }
}

/// hay 에서 query 가 시작하는 위치들 (char 단위, 겹쳐도 모두). vim 의 smartcase 처럼
/// query 에 대문자가 없으면 대소문자를 가리지 않는다
pub(super) fn find_all(hay: &[char], query: &[char]) -> Vec<usize> {
    if query.is_empty() || hay.len() < query.len() {
        return Vec::new();
    }
    let fold = !query.iter().any(|c| c.is_uppercase());
    let same = |a: char, b: char| a == b || (fold && a.to_lowercase().eq(b.to_lowercase()));
    (0..=hay.len() - query.len())
        .filter(|&i| hay[i..].iter().zip(query).all(|(&a, &b)| same(a, b)))
        .collect()
}

pub(super) fn to_u16(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

/// 가장 긴 줄의 화면 폭
pub(super) fn lines_width(lines: &[Line]) -> u16 {
    to_u16(lines.iter().map(Line::width).max().unwrap_or(0))
}

/// 글자 하나의 화면 폭 (한글 2칸)
pub(super) fn char_width(c: char) -> usize {
    Span::raw(c.encode_utf8(&mut [0; 4]) as &str).width()
}

/// 앞 chars 글자의 화면 폭 (한글 2칸)
pub(super) fn prefix_width(s: &str, chars: usize) -> usize {
    let end = s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i);
    Span::raw(&s[..end]).width()
}

/// 줄을 width 칸 안으로 줄인다. 넘치면 끝을 잘라 잘린 자리의 스타일로 … 를 붙인다
/// (경계에 걸린 2칸 문자는 통째로 뺀다)
pub(super) fn truncate(mut line: Line<'_>, width: u16) -> Line<'_> {
    let width = usize::from(width);
    if line.width() <= width {
        return line;
    }
    let mut room = width.saturating_sub(1);
    let mut spans = Vec::new();
    let mut cut = Style::new();
    for span in std::mem::take(&mut line.spans) {
        if span.width() <= room {
            room -= span.width();
            spans.push(span);
            continue;
        }
        let mut kept = String::new();
        for c in span.content.chars() {
            let w = char_width(c);
            if w > room {
                break;
            }
            room -= w;
            kept.push(c);
        }
        cut = span.style;
        if !kept.is_empty() {
            spans.push(Span::styled(kept, span.style));
        }
        break;
    }
    if width > 0 {
        spans.push(Span::styled("…", cut));
    }
    line.spans = spans;
    line
}

/// 탭 멈춤 간격. 예제 편집기의 Tab 키가 넣는 공백 수(4)와 같다
const TAB: usize = 4;

/// 코드 한 줄의 글자마다 (펼친 칸 수, 칸 하나의 폭). 탭은 다음 탭 멈춤까지 1칸짜리 공백 여러 개,
/// 그 밖의 글자는 자기 폭 1개. expand_tabs 와 wrap_cursor 가 같은 규칙을 쓰게 한 곳에 둔다
fn cells(chars: impl Iterator<Item = char>) -> impl Iterator<Item = (char, usize, usize)> {
    let mut col = 0;
    chars.map(move |c| {
        let (count, w) = if c == '\t' {
            (TAB - col % TAB, 1)
        } else {
            (1, char_width(c))
        };
        col += count * w;
        (c, count, w)
    })
}

/// 탭을 TAB 칸 간격의 탭 멈춤까지 공백으로 펼친다 (ratatui 는 탭 같은 제어 문자를 그리지 않는다)
pub(super) fn expand_tabs(spans: Vec<Span<'static>>) -> Vec<Span<'static>> {
    if !spans.iter().any(|s| s.content.contains('\t')) {
        return spans;
    }
    // 탭 멈춤은 줄 처음부터 세므로 span 을 이어서 센다
    let mut chars = cells(spans.iter().flat_map(|s| s.content.chars()));
    spans
        .iter()
        .map(|span| {
            let text: String = chars
                .by_ref()
                .take(span.content.chars().count())
                .flat_map(|(c, count, _)| {
                    std::iter::repeat_n(if c == '\t' { ' ' } else { c }, count)
                })
                .collect();
            Span::styled(text, span.style)
        })
        .collect()
}

/// wrap_cells 가 줄을 끊는 자리: 이미 놓인 글자가 있고 다음 글자(폭 w)까지 놓으면 width 를 넘을 때
fn breaks(used: usize, w: usize, width: usize) -> bool {
    width > 0 && used > 0 && used + w > width
}

/// 코드 한 줄을 `wrap_cells(expand_tabs(..), width)` 로 그렸을 때 at 번째 글자가 놓이는 (접힌 줄, 칸).
/// at 이 글자 수 이상이면(Insert 의 줄 끝) 마지막 글자 바로 뒤 칸이다. 탭은 펼친 첫 칸
pub(super) fn wrap_cursor(line: &str, at: usize, width: usize) -> (usize, usize) {
    let (mut row, mut used) = (0, 0);
    for (i, (_, count, w)) in cells(line.chars()).enumerate() {
        for k in 0..count {
            if breaks(used, w, width) {
                row += 1;
                used = 0;
            }
            if i == at && k == 0 {
                return (row, used);
            }
            used += w;
        }
    }
    (row, used)
}

/// 설명문 줄들을 width 칸 안에 접는다. ratatui `Wrap { trim: false }` 와 같은 규칙으로 공백에서 끊고
/// (줄 끝에 걸린 공백은 버리고 들여쓰기는 유지), 폭보다 긴 단어는 글자 단위로 끊는다.
/// ratatui 0.30 은 다음 글자 폭을 빼고 넘침을 판단해 2칸 문자가 마지막 칸에 걸리면 1칸 넘치는데,
/// 여기서는 그 글자를 다음 줄로 넘겨 width 를 넘지 않는다 (폭이 1칸이면 2칸 문자는 한 줄에 하나).
/// 줄·span 스타일과 정렬은 그대로 둔다
pub(super) fn wrap_lines(lines: &[Line], width: u16) -> Vec<Line<'static>> {
    lines
        .iter()
        .flat_map(|line| wrap_line(line, width))
        .collect()
}

fn wrap_line(line: &Line, width: u16) -> Vec<Line<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let mut rows: Vec<Vec<StyledGrapheme>> = Vec::new();
    let mut row = Vec::new();
    let mut row_w = 0;
    // 아직 줄에 붙이지 않은 공백과 그 뒤 단어
    let mut space = VecDeque::new();
    let mut space_w = 0;
    let mut word = Vec::new();
    let mut word_w = 0;
    let mut after_word = false;
    for g in line
        .spans
        .iter()
        .flat_map(|s| s.styled_graphemes(Style::new()))
    {
        let blank = g.is_whitespace();
        let w = g.symbol.cell_width();
        // 단어가 끝났거나 빈 줄에도 다 들어가지 않으면 (글자 단위로 끊기) 지금까지를 줄에 붙인다
        if (after_word && blank) || (row.is_empty() && space_w + word_w + w > width) {
            row.extend(space.drain(..));
            row.append(&mut word);
            row_w += space_w + word_w;
            (space_w, word_w) = (0, 0);
        }
        // 줄이 찼거나 이 글자까지 놓으면 넘치면 줄을 끝내고, 줄 끝에 걸린 공백은 남은 칸만큼 버린다
        let full = row_w >= width || (w > 0 && row_w + space_w + word_w + w > width);
        if full && !row.is_empty() {
            let mut room = width.saturating_sub(row_w);
            rows.push(std::mem::take(&mut row));
            row_w = 0;
            while let Some(front) = space.front() {
                let fw = front.symbol.cell_width();
                if fw > room {
                    break;
                }
                (space_w, room) = (space_w - fw, room - fw);
                space.pop_front();
            }
            if blank && space.is_empty() {
                continue;
            }
        }
        if blank {
            space_w += w;
            space.push_back(g);
        } else {
            word_w += w;
            word.push(g);
        }
        after_word = !blank;
    }
    row.extend(space);
    row.append(&mut word);
    if !row.is_empty() || rows.is_empty() {
        rows.push(row);
    }
    rows.into_iter()
        .map(|row| {
            let mut spans: Vec<Span<'static>> = Vec::new();
            for g in row {
                match spans.last_mut() {
                    Some(last) if last.style == g.style => last.content.to_mut().push_str(g.symbol),
                    _ => spans.push(Span::styled(g.symbol.to_string(), g.style)),
                }
            }
            Line {
                spans,
                style: line.style,
                alignment: line.alignment,
            }
        })
        .collect()
}

/// 코드 한 줄을 width 칸마다 글자 단위로 접는다 (nvim 의 `wrap` 처럼).
/// 2칸 문자가 마지막 1칸에 걸리면 다음 줄로 넘겨 width 를 넘지 않는다
pub(super) fn wrap_cells(spans: Vec<Span<'static>>, width: usize) -> Vec<Line<'static>> {
    if width == 0 {
        return vec![Line::from(spans)];
    }
    let mut lines = Vec::new();
    let mut line = Vec::new();
    let mut used = 0;
    for span in spans {
        let mut piece = String::new();
        for c in span.content.chars() {
            let w = char_width(c);
            if breaks(used, w, width) {
                if !piece.is_empty() {
                    line.push(Span::styled(std::mem::take(&mut piece), span.style));
                }
                lines.push(Line::from(std::mem::take(&mut line)));
                used = 0;
            }
            piece.push(c);
            used += w;
        }
        if !piece.is_empty() {
            line.push(Span::styled(piece, span.style));
        }
    }
    lines.push(Line::from(line));
    lines
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Alignment;
    use ratatui::style::{Color, Style, Stylize};
    use ratatui::text::{Line, Span};

    use super::{char_width, expand_tabs, find_all, truncate, wrap_cells, wrap_cursor, wrap_lines};

    #[test]
    fn find_all_is_smartcase() {
        let find = |hay: &str, q: &str| {
            let hay: Vec<char> = hay.chars().collect();
            find_all(&hay, &q.chars().collect::<Vec<_>>())
        };
        assert_eq!(find("Main main MAIN", "main"), [0, 5, 10]);
        assert_eq!(find("Main main MAIN", "Main"), [0]);
        assert_eq!(find("가나가나", "가나"), [0, 2]);
        assert_eq!(find("aaa", "aa"), [0, 1]);
        assert!(find("ab", "").is_empty() && find("a", "ab").is_empty());
    }

    fn wrap(text: &str, width: u16) -> Vec<String> {
        wrap_lines(&[Line::from(text)], width)
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn wrap_lines_never_overflows_wide_chars() {
        // ratatui 0.30 Wrap 은 "aaa 가" 를 폭 5 에 6칸 한 줄로 넘치게 두지만 여기서는 다음 줄로
        assert_eq!(wrap("aaa 가", 5), ["aaa", "가"]);
        // 공백에서 끊고(줄 끝 공백은 버림) 들여쓰기는 유지, 폭보다 긴 단어는 글자 단위
        assert_eq!(wrap("  ab cd", 5), ["  ab", "cd"]);
        assert_eq!(wrap("가나다라", 5), ["가나", "다라"]);
        // 폭 1 이면 2칸 문자는 버리지 않고 한 줄에 하나, 폭 0 이면 줄 없음, 빈 줄은 빈 줄 하나
        assert_eq!(wrap("a가", 1), ["a", "가"]);
        assert!(wrap("a", 0).is_empty());
        assert_eq!(wrap("", 5), [""]);

        // span 스타일과 줄 스타일·정렬은 그대로
        let line = Line::from(vec!["ab ".into(), "cd".red()]).bold().centered();
        let lines = wrap_lines(&[line], 3);
        let texts: Vec<String> = lines.iter().map(ToString::to_string).collect();
        assert_eq!(texts, ["ab", "cd"]);
        assert_eq!(lines[1].spans[0].style, Style::new().red());
        for line in &lines {
            assert_eq!(
                (line.style, line.alignment),
                (Style::new().bold(), Some(Alignment::Center))
            );
        }
    }

    #[test]
    fn wrap_cells_never_exceeds_width() {
        let spans = vec![Span::raw("ab가"), "나c".fg(Color::Red)];
        let text =
            |lines: &[Line]| -> Vec<String> { lines.iter().map(ToString::to_string).collect() };
        // 2칸 문자가 마지막 1칸에 걸리면 다음 줄로, 색은 그대로
        let lines = wrap_cells(spans.clone(), 3);
        assert_eq!(text(&lines), ["ab", "가", "나c"]);
        assert_eq!(lines[2].spans[0].style.fg, Some(Color::Red));
        assert_eq!(text(&wrap_cells(spans.clone(), 4)), ["ab가", "나c"]);
        assert_eq!(text(&wrap_cells(spans.clone(), 99)), ["ab가나c"]);
        // 1칸에도 2칸 문자는 한 줄에 하나씩 (빈 줄 없이)
        assert_eq!(
            text(&wrap_cells(spans.clone(), 1)),
            ["a", "b", "가", "나", "c"]
        );
        assert_eq!(text(&wrap_cells(spans, 0)), ["ab가나c"]);
        assert_eq!(text(&wrap_cells(Vec::new(), 5)), [""]);
    }

    #[test]
    fn expand_tabs_uses_stops_across_spans() {
        let text = |spans: Vec<Span<'static>>| Line::from(expand_tabs(spans)).to_string();
        assert_eq!(text(vec![Span::raw("\tx")]), "    x");
        // 탭 멈춤은 줄 처음부터 센다 (2칸 문자 포함, span 이 나뉘어도)
        assert_eq!(text(vec![Span::raw("ab\tc")]), "ab  c");
        assert_eq!(text(vec![Span::raw("가"), Span::raw("a\t|")]), "가a |");
        assert_eq!(text(vec![Span::raw("abcd"), "\t".red()]), "abcd    ");
        // 탭이 든 span 의 스타일은 그대로
        let spans = expand_tabs(vec![Span::raw("a"), "\tb".red()]);
        assert_eq!(spans[1].content, "   b");
        assert_eq!(spans[1].style, Style::new().red());
    }

    /// 줄을 wrap_cells 로 그린 화면에서 글자마다 (접힌 줄, 칸, 글자) — 2칸 문자 뒤 칸은 없음
    fn drawn(line: &str, width: usize) -> Vec<(usize, usize, char)> {
        let rows = wrap_cells(expand_tabs(vec![Span::raw(line.to_string())]), width);
        let mut cells = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let mut x = 0;
            for c in row.to_string().chars() {
                cells.push((r, x, c));
                x += char_width(c);
            }
        }
        cells
    }

    #[test]
    fn wrap_cursor_matches_wrap_cells() {
        // 커서 자리는 화면에 그려진 그 글자 자리와 같다 (탭은 펼친 첫 칸, 줄 끝은 마지막 글자 뒤)
        for line in [
            "ab가나c",
            "\tx\t가",
            "가나다라마바",
            "a\tb\t\tc가d",
            "",
            "    let x;",
        ] {
            for width in 0..9 {
                let cells = drawn(line, width);
                let mut i = 0;
                for (at, (c, count, _)) in super::cells(line.chars()).enumerate() {
                    let (r, x, drawn) = cells[i];
                    assert_eq!(
                        wrap_cursor(line, at, width),
                        (r, x),
                        "{line:?} {width} {at}"
                    );
                    assert_eq!(drawn, if c == '\t' { ' ' } else { c });
                    i += count;
                }
                let end = cells
                    .last()
                    .map_or((0, 0), |&(r, x, c)| (r, x + char_width(c)));
                let n = line.chars().count();
                assert_eq!(wrap_cursor(line, n, width), end, "{line:?} {width} end");
                assert_eq!(wrap_cursor(line, usize::MAX, width), end);
            }
        }
        // 폭 4: "ab가나c" → "ab가" / "나c", 꽉 찬 줄 끝 뒤 커서는 오른쪽 여백 칸(폭과 같은 칸)
        assert_eq!(wrap_cursor("ab가나c", 3, 4), (1, 0));
        assert_eq!(wrap_cursor("abcd", 4, 4), (0, 4));
        assert_eq!(wrap_cursor("\tx", 1, 80), (0, 4));
    }

    #[test]
    fn truncate_ends_with_ellipsis_in_cut_style() {
        let line = Line::from(vec![
            Span::raw(" 1. "),
            Span::raw("제목"),
            Span::styled("  요약 summary", Style::new().red()),
        ]);
        // 들어가면 그대로
        assert_eq!(truncate(line.clone(), 40), line);
        // 잘린 자리(요약)의 스타일로 … 를 붙여 폭에 맞춘다
        let cut = truncate(line.clone(), 10);
        assert_eq!(cut.to_string(), " 1. 제목 …");
        assert_eq!(cut.width(), 10);
        assert_eq!(cut.spans.last().unwrap().style, Style::new().red());
        // 경계에 걸린 2칸 문자는 통째로 빼고, 폭 0 이면 빈 줄
        assert_eq!(truncate(line.clone(), 7).to_string(), " 1. 제…");
        assert_eq!(truncate(line.clone(), 6).to_string(), " 1. …");
        assert!(truncate(line, 0).spans.is_empty());
    }
}
