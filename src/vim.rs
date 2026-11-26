//! vim 스타일 모달 편집기 (std 만 사용). 커서 col 은 바이트가 아닌 char 단위.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Esc,
    Enter,
    Backspace,
    Delete,
    Tab,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    /// Insert 의 Ctrl+W: 커서 앞 단어 지우기
    WordBackspace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
}

type Snapshot = (Vec<String>, usize, usize);

const UNDO_LIMIT: usize = 100;

#[derive(Clone, Debug)]
pub struct Editor {
    lines: Vec<String>,
    row: usize,
    col: usize,
    mode: Mode,
    multiline: bool,
    // 다음 키를 기다리는 연산자와 그 뒤에 i 를 눌렀는지 (ciw/diw)
    pending: Option<(char, bool)>,
    register: Option<String>,
    undo: Vec<Snapshot>,
}

impl Editor {
    pub fn new(text: &str, multiline: bool, mode: Mode) -> Self {
        let lines = if multiline {
            text.split('\n').map(String::from).collect()
        } else {
            vec![text.replace('\n', " ")]
        };
        let mut e = Self {
            lines,
            row: 0,
            col: 0,
            mode,
            multiline,
            pending: None,
            register: None,
            undo: Vec::new(),
        };
        if mode == Mode::Insert {
            e.row = e.lines.len() - 1;
            e.col = e.len(e.row);
            e.undo.push(e.snapshot());
        }
        e
    }

    /// 편집기가 처리했으면 true, 앱이 처리할 키면 false (상태는 그대로).
    /// Normal 에서는 `d`/`c`/`r`/`y`/`g`(`ci`/`di`) 다음 키를 기다리는 중이 아니면서 매핑이 없는 키가 false,
    /// Insert 에서는 한 줄 편집기의 `Enter`/`Tab`/`↑`/`↓` 만 false.
    pub fn key(&mut self, key: Key) -> bool {
        if self.mode == Mode::Insert {
            return self.insert_key(key);
        }
        if key == Key::Char('u') && self.pending.is_none() {
            if let Some((lines, row, col)) = self.undo.pop() {
                (self.lines, self.row, self.col) = (lines, row, col);
            }
            self.clamp();
            return true;
        }
        let before = self.snapshot();
        if !self.normal_key(key) {
            return false;
        }
        // Insert 로 들어가면 Esc 까지 한 단위. 변화가 없었으면 Esc 에서 버린다.
        if self.mode == Mode::Insert || self.lines != before.0 {
            if self.undo.len() >= UNDO_LIMIT {
                self.undo.remove(0);
            }
            self.undo.push(before);
        }
        self.clamp();
        true
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// 다음 키를 기다리는 연산자(d/c/r 등)를 취소한다 (모드는 그대로)
    pub fn cancel_pending(&mut self) {
        self.pending = None;
    }

    /// 커서를 (row, col) 로 옮긴다 (줄과 글자 수로 잘린다). 검색 이동에 쓴다
    pub fn set_cursor(&mut self, row: usize, col: usize) {
        (self.row, self.col) = (row, col);
        self.clamp();
    }

    fn insert_key(&mut self, key: Key) -> bool {
        let multi = self.multiline;
        match key {
            Key::Esc => {
                self.mode = Mode::Normal;
                self.col = self.col.saturating_sub(1);
                if self.undo.last().is_some_and(|(l, ..)| *l == self.lines) {
                    self.undo.pop();
                }
            }
            Key::Enter | Key::Tab | Key::Up | Key::Down if !multi => return false,
            Key::Char(c) => {
                if !c.is_control() {
                    self.insert_text(c.encode_utf8(&mut [0; 4]));
                }
            }
            Key::Tab => self.insert_text("    "),
            Key::Enter => {
                let line = &mut self.lines[self.row];
                let rest = line.split_off(byte(line, self.col));
                let ind = indent(line);
                self.col = ind.chars().count();
                self.row += 1;
                self.lines.insert(self.row, ind + &rest);
            }
            Key::Backspace if self.col > 0 => {
                self.col -= 1;
                self.splice(self.col, self.col + 1, "");
            }
            Key::Backspace if multi && self.row > 0 => {
                let line = self.lines.remove(self.row);
                self.row -= 1;
                self.col = self.len(self.row);
                self.lines[self.row].push_str(&line);
            }
            Key::Delete if self.col < self.len(self.row) => self.splice(self.col, self.col + 1, ""),
            Key::Delete if multi && self.row + 1 < self.lines.len() => {
                let next = self.lines.remove(self.row + 1);
                self.lines[self.row].push_str(&next);
            }
            Key::Backspace | Key::Delete => {}
            Key::WordBackspace if self.col > 0 => {
                let line = self.chars(self.row);
                let mut from = run_start(&line, self.col, 0);
                if from > 0 {
                    from = run_start(&line, from, class(line[from - 1]));
                }
                self.splice(from, self.col, "");
                self.col = from;
            }
            // 줄 처음이면 Backspace 처럼 (예제는 윗줄과 합친다)
            Key::WordBackspace => return self.insert_key(Key::Backspace),
            Key::Left => self.col = self.col.saturating_sub(1),
            Key::Right => self.col += 1,
            Key::Home => self.col = 0,
            Key::End => self.col = usize::MAX,
            Key::Up => self.row = self.row.saturating_sub(1),
            Key::Down => self.row += 1,
        }
        self.clamp();
        true
    }

    fn normal_key(&mut self, key: Key) -> bool {
        if let Some((op, inner)) = self.pending.take() {
            match key {
                Key::Char('w') if inner => self.inner_word(op),
                Key::Char('i') if !inner && matches!(op, 'c' | 'd') => {
                    self.pending = Some((op, true));
                }
                Key::Char(c) if !inner => self.operator(op, c),
                _ => {}
            }
            return true;
        }
        let multi = self.multiline;
        match key {
            Key::Char('h') | Key::Left | Key::Backspace => self.col = self.col.saturating_sub(1),
            Key::Char('l') | Key::Right => self.col += 1,
            Key::Char('0') | Key::Home => self.col = 0,
            Key::Char('^') => self.col = first_non_blank(&self.lines[self.row]),
            Key::Char('$') | Key::End => self.col = usize::MAX,
            Key::Char('w') => (self.row, self.col) = self.next_word(),
            Key::Char('b') => (self.row, self.col) = self.prev_word(),
            Key::Char('e') => (self.row, self.col) = self.word_end(),
            Key::Char('j') | Key::Down if multi => self.row += 1,
            Key::Char('k') | Key::Up if multi => self.row = self.row.saturating_sub(1),
            Key::Enter if multi => self.goto_line(self.row + 1),
            Key::Char('i') => self.mode = Mode::Insert,
            Key::Char('a') => {
                self.mode = Mode::Insert;
                self.col += 1;
            }
            Key::Char('I') => {
                self.mode = Mode::Insert;
                self.col = first_non_blank(&self.lines[self.row]);
            }
            Key::Char('A') => {
                self.mode = Mode::Insert;
                self.col = usize::MAX;
            }
            Key::Char('x') | Key::Delete => self.splice(self.col, self.col + 1, ""),
            Key::Char('D') => self.splice(self.col, usize::MAX, ""),
            Key::Char('C') => {
                self.splice(self.col, usize::MAX, "");
                self.mode = Mode::Insert;
            }
            Key::Char('s') => {
                self.splice(self.col, self.col + 1, "");
                self.mode = Mode::Insert;
            }
            Key::Char('S') => self.change_line(),
            Key::Char(c @ ('d' | 'c' | 'r')) => self.pending = Some((c, false)),
            Key::Char(c @ ('y' | 'g')) if multi => self.pending = Some((c, false)),
            Key::Char('G') if multi => self.goto_line(self.lines.len() - 1),
            Key::Char('o') if multi => self.open_line(self.row + 1),
            Key::Char('O') if multi => self.open_line(self.row),
            Key::Char('p') if multi => self.paste(self.row + 1),
            Key::Char('P') if multi => self.paste(self.row),
            _ => return false,
        }
        true
    }

    fn operator(&mut self, op: char, c: char) {
        match (op, c) {
            ('r', _) if !c.is_control() && self.col < self.len(self.row) => {
                self.splice(self.col, self.col + 1, c.encode_utf8(&mut [0; 4]));
            }
            ('d', 'd') => {
                let line = if self.lines.len() > 1 {
                    self.lines.remove(self.row)
                } else {
                    std::mem::take(&mut self.lines[0])
                };
                self.register = Some(line);
                self.goto_line(self.row);
            }
            ('d' | 'c', 'w') => {
                let end = word_span(&self.chars(self.row), self.col, op == 'c');
                self.splice(self.col, end, "");
                if op == 'c' {
                    self.mode = Mode::Insert;
                }
            }
            ('c', 'c') => self.change_line(),
            ('y', 'y') => self.register = Some(self.lines[self.row].clone()),
            ('g', 'g') => self.goto_line(0),
            _ => {}
        }
    }

    /// ciw/diw: 커서 아래 단어(공백 위면 이어진 공백)를 지운다. 빈 줄에서도 ciw 는 Insert.
    fn inner_word(&mut self, op: char) {
        let line = self.chars(self.row);
        if let Some(&ch) = line.get(self.col) {
            let k = class(ch);
            let mut from = self.col;
            while from > 0 && class(line[from - 1]) == k {
                from -= 1;
            }
            self.splice(from, run_end(&line, self.col, k), "");
            self.col = from;
        }
        if op == 'c' {
            self.mode = Mode::Insert;
        }
    }

    /// row 줄(줄 수로 잘림)의 첫 글자로 간다.
    fn goto_line(&mut self, row: usize) {
        self.row = row.min(self.lines.len() - 1);
        self.col = first_non_blank(&self.lines[self.row]);
    }

    /// 들여쓰기는 남기고 줄 내용을 지운 뒤 Insert.
    fn change_line(&mut self) {
        let ind = indent(&self.lines[self.row]);
        self.col = ind.chars().count();
        self.lines[self.row] = ind;
        self.mode = Mode::Insert;
    }

    fn open_line(&mut self, at: usize) {
        self.lines.insert(at, indent(&self.lines[self.row]));
        self.row = at;
        self.change_line();
    }

    fn paste(&mut self, at: usize) {
        if let Some(line) = self.register.clone() {
            self.col = first_non_blank(&line);
            self.lines.insert(at, line);
            self.row = at;
        }
    }

    /// 줄 끝에서는 다음 줄의 첫 글자로 넘어가고, 빈 줄에서 멈춘다.
    fn next_word(&self) -> (usize, usize) {
        let mut r = self.row;
        let mut c = word_span(&self.chars(r), self.col, false);
        while c >= self.len(r) && self.multiline && r + 1 < self.lines.len() {
            r += 1;
            if self.lines[r].is_empty() {
                return (r, 0);
            }
            c = first_non_blank(&self.lines[r]);
        }
        (r, c)
    }

    fn word_end(&self) -> (usize, usize) {
        let (mut r, mut c) = (self.row, self.col + 1);
        loop {
            let line = self.chars(r);
            c = run_end(&line, c, 0);
            if let Some(&ch) = line.get(c) {
                return (r, run_end(&line, c, class(ch)) - 1);
            }
            if !self.multiline || r + 1 == self.lines.len() {
                return (self.row, self.col);
            }
            r += 1;
            c = 0;
        }
    }

    fn prev_word(&self) -> (usize, usize) {
        let (mut r, mut c) = (self.row, self.col);
        loop {
            let line = self.chars(r);
            let mut i = c.min(line.len());
            while i > 0 && class(line[i - 1]) == 0 {
                i -= 1;
            }
            if i > 0 {
                let k = class(line[i - 1]);
                while i > 0 && class(line[i - 1]) == k {
                    i -= 1;
                }
                return (r, i);
            }
            if !self.multiline || r == 0 {
                return (r, 0);
            }
            r -= 1;
            c = self.len(r);
            if c == 0 {
                return (r, 0);
            }
        }
    }

    fn insert_text(&mut self, text: &str) {
        self.splice(self.col, self.col, text);
        self.col += text.chars().count();
    }

    /// 현재 줄의 char 범위 [from, to) 를 text 로 바꾼다. 범위는 줄 길이로 잘린다.
    fn splice(&mut self, from: usize, to: usize, text: &str) {
        let line = &mut self.lines[self.row];
        let a = byte(line, from);
        let b = byte(line, to.max(from));
        line.replace_range(a..b, text);
    }

    fn clamp(&mut self) {
        self.row = self.row.min(self.lines.len() - 1);
        let len = self.len(self.row);
        let max = match self.mode {
            Mode::Insert => len,
            Mode::Normal => len.saturating_sub(1),
        };
        self.col = self.col.min(max);
    }

    fn snapshot(&self) -> Snapshot {
        (self.lines.clone(), self.row, self.col)
    }

    fn chars(&self, r: usize) -> Vec<char> {
        self.lines[r].chars().collect()
    }

    fn len(&self, r: usize) -> usize {
        self.lines[r].chars().count()
    }
}

fn byte(s: &str, ci: usize) -> usize {
    s.char_indices().nth(ci).map_or(s.len(), |(b, _)| b)
}

/// 0: 공백, 1: 단어(영숫자·_, 한글 포함), 2: 구두점
fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

fn indent(s: &str) -> String {
    s.chars().take_while(|c| c.is_whitespace()).collect()
}

fn first_non_blank(s: &str) -> usize {
    s.chars().take_while(|c| c.is_whitespace()).count()
}

/// dw/cw 가 지울 줄 안의 끝 위치(exclusive). cw 는 단어 위에서 뒤 공백을 남긴다.
fn word_span(line: &[char], c: usize, change: bool) -> usize {
    let Some(&ch) = line.get(c) else {
        return c;
    };
    let e = run_end(line, c, class(ch));
    if change && class(ch) != 0 {
        e
    } else {
        run_end(line, e, 0)
    }
}

/// i 바로 앞에서부터 class 가 k 인 글자가 이어지는 구간의 시작
fn run_start(line: &[char], mut i: usize, k: u8) -> usize {
    while i > 0 && class(line[i - 1]) == k {
        i -= 1;
    }
    i
}

/// i 부터 class 가 k 인 글자가 이어지는 구간의 끝(exclusive)
fn run_end(line: &[char], mut i: usize, k: u8) -> usize {
    while line.get(i).is_some_and(|&x| class(x) == k) {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typ(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.key(Key::Char(c));
        }
    }

    fn normal(text: &str, multi: bool) -> Editor {
        Editor::new(text, multi, Mode::Normal)
    }

    fn check(e: &Editor) {
        assert!(!e.lines().is_empty());
        assert!(e.multiline || e.lines().len() == 1);
        let (r, c) = e.cursor();
        assert!(r < e.lines().len());
        let len = e.lines()[r].chars().count();
        match e.mode() {
            Mode::Insert => assert!(c <= len),
            Mode::Normal => assert!(c <= len.saturating_sub(1)),
        }
        assert!(e.lines().iter().all(|l| !l.contains('\n')));
    }

    type State = (Vec<String>, usize, usize, Mode, Option<(char, bool)>, usize);

    fn state(e: &Editor) -> State {
        (
            e.lines.clone(),
            e.row,
            e.col,
            e.mode,
            e.pending,
            e.undo.len(),
        )
    }

    fn all_keys() -> Vec<Key> {
        let mut v = vec![
            Key::Esc,
            Key::Enter,
            Key::Backspace,
            Key::Delete,
            Key::Tab,
            Key::Left,
            Key::Right,
            Key::Up,
            Key::Down,
            Key::Home,
            Key::End,
            Key::WordBackspace,
            Key::Char('가'),
            Key::Char('\n'),
        ];
        v.extend((' '..='~').map(Key::Char));
        v
    }

    #[test]
    fn new_cursor_positions() {
        assert_eq!(normal("ab\ncd", true).cursor(), (0, 0));
        let e = Editor::new("ab\ncde", true, Mode::Insert);
        assert_eq!((e.cursor(), e.mode()), ((1, 3), Mode::Insert));
        assert_eq!(normal("a\nb", false).lines(), ["a b"]);
        assert_eq!(normal("a\n\nb\n", true).text(), "a\n\nb\n");
    }

    #[test]
    fn insert_typing_and_esc_moves_left() {
        let mut e = Editor::new("ab", false, Mode::Insert);
        assert_eq!(e.cursor(), (0, 2));
        typ(&mut e, "cd");
        assert_eq!((e.text().as_str(), e.cursor()), ("abcd", (0, 4)));
        assert!(e.key(Key::Esc));
        assert_eq!((e.mode(), e.cursor()), (Mode::Normal, (0, 3)));

        let mut e = Editor::new("ab", false, Mode::Insert);
        e.key(Key::Home);
        e.key(Key::Esc);
        assert_eq!(e.cursor(), (0, 0));
    }

    #[test]
    fn insert_edit_keys() {
        let mut e = Editor::new("abc", false, Mode::Insert);
        e.key(Key::Left);
        e.key(Key::Backspace);
        assert_eq!((e.text().as_str(), e.cursor()), ("ac", (0, 1)));
        e.key(Key::Home);
        e.key(Key::Delete);
        assert_eq!((e.text().as_str(), e.cursor()), ("c", (0, 0)));
        e.key(Key::Backspace);
        e.key(Key::Right);
        e.key(Key::Right);
        typ(&mut e, "d");
        e.key(Key::End);
        typ(&mut e, "e");
        assert_eq!(e.text(), "cde");
    }

    #[test]
    fn insert_commands_i_a_shift_i_shift_a() {
        let mut e = normal("  ab", false);
        typ(&mut e, "$ix");
        assert_eq!(e.text(), "  axb");
        e.key(Key::Esc);
        typ(&mut e, "ay");
        assert_eq!(e.text(), "  axyb");
        e.key(Key::Esc);
        typ(&mut e, "I<");
        assert_eq!(e.text(), "  <axyb");
        e.key(Key::Esc);
        typ(&mut e, "A>");
        assert_eq!(e.text(), "  <axyb>");
    }

    #[test]
    fn cursor_motions_h_l_0_caret_dollar() {
        let mut e = normal("  abc", false);
        typ(&mut e, "$");
        assert_eq!(e.cursor(), (0, 4));
        typ(&mut e, "l");
        assert_eq!(e.cursor(), (0, 4));
        typ(&mut e, "h");
        assert_eq!(e.cursor(), (0, 3));
        typ(&mut e, "0");
        assert_eq!(e.cursor(), (0, 0));
        typ(&mut e, "^");
        assert_eq!(e.cursor(), (0, 2));
        e.key(Key::End);
        e.key(Key::Left);
        assert_eq!(e.cursor(), (0, 3));
    }

    #[test]
    fn x_deletes_char_and_clamps() {
        let mut e = normal("abc", false);
        typ(&mut e, "lx");
        assert_eq!((e.text().as_str(), e.cursor()), ("ac", (0, 1)));
        typ(&mut e, "x");
        assert_eq!((e.text().as_str(), e.cursor()), ("a", (0, 0)));
    }

    #[test]
    fn shift_d_and_shift_c() {
        let mut e = normal("hello world", false);
        typ(&mut e, "wD");
        assert_eq!((e.text().as_str(), e.cursor()), ("hello ", (0, 5)));

        let mut e = normal("hello world", false);
        typ(&mut e, "wCx");
        e.key(Key::Esc);
        assert_eq!((e.text().as_str(), e.mode()), ("hello x", Mode::Normal));
    }

    #[test]
    fn s_and_shift_s_and_cc() {
        let mut e = normal("abc", false);
        typ(&mut e, "sX");
        e.key(Key::Esc);
        assert_eq!(e.text(), "Xbc");

        let mut e = normal("    foo\nbar", true);
        typ(&mut e, "S");
        assert_eq!((e.lines()[0].as_str(), e.cursor()), ("    ", (0, 4)));
        assert_eq!(e.mode(), Mode::Insert);
        typ(&mut e, "x");
        e.key(Key::Esc);
        typ(&mut e, "jcc");
        assert_eq!(e.text(), "    x\n");
        assert_eq!(e.mode(), Mode::Insert);
    }

    #[test]
    fn dd_multiline_and_single() {
        let mut e = normal("a\n  b\nc", true);
        typ(&mut e, "jdd");
        assert_eq!((e.text().as_str(), e.cursor()), ("a\nc", (1, 0)));
        typ(&mut e, "dd");
        assert_eq!((e.text().as_str(), e.cursor()), ("a", (0, 0)));
        typ(&mut e, "dd");
        assert_eq!(e.text(), "");

        let mut e = normal("abc", false);
        typ(&mut e, "ldd");
        assert_eq!((e.text().as_str(), e.cursor()), ("", (0, 0)));
    }

    #[test]
    fn dw_and_cw() {
        let mut e = normal("let mut x", false);
        typ(&mut e, "dw");
        assert_eq!(e.text(), "mut x");

        let mut e = normal("ab cd", false);
        typ(&mut e, "wdw");
        assert_eq!((e.text().as_str(), e.cursor()), ("ab ", (0, 2)));

        let mut e = normal("let mut x", false);
        typ(&mut e, "wcwfoo");
        e.key(Key::Esc);
        assert_eq!(e.text(), "let foo x");

        let mut e = normal("a.b", false);
        typ(&mut e, "dw");
        assert_eq!(e.text(), ".b");
    }

    #[test]
    fn r_replaces_and_pending_esc_cancels() {
        let mut e = normal("abc", false);
        typ(&mut e, "lrX");
        assert_eq!((e.text().as_str(), e.cursor()), ("aXc", (0, 1)));

        let mut e = normal("abc", false);
        typ(&mut e, "d");
        assert!(e.key(Key::Esc));
        typ(&mut e, "x");
        assert_eq!(e.text(), "bc");
        typ(&mut e, "r");
        assert!(e.key(Key::Esc));
        typ(&mut e, "c");
        assert!(e.key(Key::Esc));
        assert_eq!((e.text().as_str(), e.mode()), ("bc", Mode::Normal));
    }

    #[test]
    fn undo() {
        let mut e = normal("abc", false);
        typ(&mut e, "xx");
        assert_eq!(e.text(), "c");
        typ(&mut e, "u");
        assert_eq!(e.text(), "bc");
        typ(&mut e, "uu");
        assert_eq!(e.text(), "abc");

        typ(&mut e, "Ade");
        e.key(Key::Esc);
        typ(&mut e, "u");
        assert_eq!(e.text(), "abc");

        // 변화 없는 i~Esc 는 undo 단위를 만들지 않는다.
        typ(&mut e, "x");
        typ(&mut e, "i");
        e.key(Key::Esc);
        typ(&mut e, "u");
        assert_eq!(e.text(), "abc");

        // cw + 입력은 한 단위
        let mut e = normal("let mut x", false);
        typ(&mut e, "wcwfoo");
        e.key(Key::Esc);
        typ(&mut e, "u");
        assert_eq!(e.text(), "let mut x");

        // Insert 로 시작한 편집기
        let mut e = Editor::new("ab", false, Mode::Insert);
        typ(&mut e, "c");
        e.key(Key::Esc);
        typ(&mut e, "u");
        assert_eq!(e.text(), "ab");
    }

    #[test]
    fn word_motions_with_punctuation() {
        let text = "let mut x = a.b();";
        let run = |start: &str, k: char| {
            let mut e = normal(text, false);
            typ(&mut e, start);
            (0..8)
                .map(|_| {
                    e.key(Key::Char(k));
                    e.cursor().1
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(run("", 'w'), [4, 8, 10, 12, 13, 14, 15, 17]);
        assert_eq!(run("", 'e'), [2, 6, 8, 10, 12, 13, 14, 17]);
        assert_eq!(run("$", 'b'), [15, 14, 13, 12, 10, 8, 4, 0]);
    }

    #[test]
    fn word_motions_cross_lines() {
        let mut e = normal("ab\n  cd\n\nef", true);
        let mut got = vec![];
        for _ in 0..4 {
            typ(&mut e, "w");
            got.push(e.cursor());
        }
        assert_eq!(got, [(1, 2), (2, 0), (3, 0), (3, 1)]);
        got.clear();
        for _ in 0..4 {
            typ(&mut e, "b");
            got.push(e.cursor());
        }
        assert_eq!(got, [(3, 0), (2, 0), (1, 2), (0, 0)]);
        got.clear();
        for _ in 0..3 {
            typ(&mut e, "e");
            got.push(e.cursor());
        }
        assert_eq!(got, [(0, 1), (1, 3), (3, 1)]);
    }

    #[test]
    fn j_k_gg_shift_g_enter() {
        let mut e = normal("abc\nd\n  efg", true);
        typ(&mut e, "llj");
        assert_eq!(e.cursor(), (1, 0));
        typ(&mut e, "jj");
        assert_eq!(e.cursor(), (2, 0));
        typ(&mut e, "k");
        assert_eq!(e.cursor(), (1, 0));
        e.key(Key::Up);
        assert_eq!(e.cursor(), (0, 0));
        e.key(Key::Down);
        assert_eq!(e.cursor(), (1, 0));
        typ(&mut e, "G");
        assert_eq!(e.cursor(), (2, 2));
        typ(&mut e, "gg");
        assert_eq!(e.cursor(), (0, 0));
        assert!(e.key(Key::Enter));
        assert!(e.key(Key::Enter));
        assert_eq!(e.cursor(), (2, 2));
        assert!(e.key(Key::Enter));
        assert_eq!(e.cursor(), (2, 2));
    }

    #[test]
    fn o_and_shift_o_keep_indent() {
        let mut e = normal("  fn x\ny", true);
        typ(&mut e, "o");
        assert_eq!((e.text().as_str(), e.cursor()), ("  fn x\n  \ny", (1, 2)));
        assert_eq!(e.mode(), Mode::Insert);
        typ(&mut e, "z");
        e.key(Key::Esc);
        typ(&mut e, "O");
        typ(&mut e, "w");
        assert_eq!(e.text(), "  fn x\n  w\n  z\ny");
        assert_eq!(e.cursor(), (1, 3));
    }

    #[test]
    fn yy_p_shift_p() {
        let mut e = normal("a\n  b", true);
        typ(&mut e, "p");
        assert_eq!(e.text(), "a\n  b");
        typ(&mut e, "jyyp");
        assert_eq!((e.text().as_str(), e.cursor()), ("a\n  b\n  b", (2, 2)));
        typ(&mut e, "ggP");
        assert_eq!(
            (e.text().as_str(), e.cursor()),
            ("  b\na\n  b\n  b", (0, 2))
        );
    }

    #[test]
    fn multiline_backspace_and_delete_join() {
        let mut e = Editor::new("ab\ncd", true, Mode::Insert);
        e.key(Key::Home);
        e.key(Key::Backspace);
        assert_eq!((e.text().as_str(), e.cursor()), ("abcd", (0, 2)));

        let mut e = normal("ab\ncd", true);
        typ(&mut e, "A");
        e.key(Key::Delete);
        assert_eq!((e.text().as_str(), e.cursor()), ("abcd", (0, 2)));
    }

    #[test]
    fn multiline_enter_keeps_indent_and_tab() {
        let mut e = Editor::new("    foo", true, Mode::Insert);
        assert!(e.key(Key::Enter));
        assert_eq!(e.lines(), ["    foo", "    "]);
        assert_eq!(e.cursor(), (1, 4));
        assert!(e.key(Key::Tab));
        typ(&mut e, "bar");
        assert_eq!(e.text(), "    foo\n        bar");

        let mut e = normal("  ab", true);
        typ(&mut e, "$i");
        e.key(Key::Enter);
        assert_eq!((e.text().as_str(), e.cursor()), ("  a\n  b", (1, 2)));
    }

    #[test]
    fn multiline_insert_up_down() {
        let mut e = Editor::new("abc\nd", true, Mode::Insert);
        e.key(Key::Up);
        assert_eq!(e.cursor(), (0, 1));
        e.key(Key::End);
        e.key(Key::Down);
        assert_eq!(e.cursor(), (1, 1));
    }

    #[test]
    fn single_line_returns_false_for_app_keys() {
        let mut e = normal("ab", false);
        for k in [
            Key::Enter,
            Key::Char('j'),
            Key::Char('k'),
            Key::Up,
            Key::Down,
            Key::Tab,
            Key::Esc,
        ] {
            assert!(!e.key(k), "{k:?}");
        }
        // 예제 전용 키는 한 줄 편집기에서 매핑이 없다
        for c in ['G', 'o', 'O', 'p', 'P', 'y', 'g'] {
            assert!(!e.key(Key::Char(c)), "{c}");
        }
        assert_eq!(e.text(), "ab");
        typ(&mut e, "i");
        for k in [Key::Enter, Key::Tab, Key::Up, Key::Down] {
            assert!(!e.key(k), "{k:?}");
        }
        assert_eq!((e.text().as_str(), e.mode()), ("ab", Mode::Insert));
    }

    #[test]
    fn multiline_return_values() {
        let mut e = normal("a\nb", true);
        assert!(!e.key(Key::Tab));
        assert!(!e.key(Key::Esc));
        assert!(e.key(Key::Char('j')));
        assert!(e.key(Key::Enter));
        typ(&mut e, "d");
        assert!(e.key(Key::Esc));
        assert!(!e.key(Key::Esc));
    }

    #[test]
    fn normal_unmapped_keys_return_false_without_change() {
        for multi in [false, true] {
            let mut e = normal("ab\ncd", multi);
            typ(&mut e, "l");
            let before = state(&e);
            for c in [',', '?', 'q', 'Q', ':', '1', '9', '!', ' ', '\n'] {
                assert!(!e.key(Key::Char(c)), "{c:?}");
                assert_eq!(state(&e), before, "{c:?}");
            }
        }
    }

    #[test]
    fn pending_and_insert_consume_every_char() {
        // 대기 중이면 맞는 조합은 실행, 맞지 않는 조합은 대기만 취소하고 소비
        let mut e = normal("ab", false);
        typ(&mut e, "r");
        assert!(e.key(Key::Char(',')));
        assert_eq!((e.text().as_str(), e.cursor()), (",b", (0, 0)));
        for op in ['d', 'c'] {
            e.key(Key::Char(op));
            assert!(e.key(Key::Char('?')), "{op}?");
            assert_eq!((e.text().as_str(), e.pending), (",b", None), "{op}?");
        }
        // Insert 에서는 글자로 입력
        typ(&mut e, "A");
        for c in [',', '?', 'q'] {
            assert!(e.key(Key::Char(c)), "{c}");
        }
        assert_eq!(e.text(), ",b,?q");
    }

    #[test]
    fn ctrl_w_deletes_word_before_cursor() {
        let mut e = Editor::new("let 값 = foo_bar(1)", false, Mode::Insert);
        let back = |e: &mut Editor, want: &str| {
            e.key(Key::WordBackspace);
            assert_eq!(e.text(), want);
        };
        back(&mut e, "let 값 = foo_bar(1");
        back(&mut e, "let 값 = foo_bar(");
        back(&mut e, "let 값 = foo_bar");
        back(&mut e, "let 값 = ");
        back(&mut e, "let 값 ");
        back(&mut e, "let ");
        back(&mut e, "");
        back(&mut e, "");
        // 예제는 줄 처음에서 윗줄과 합친다, Normal 에서는 처리하지 않는다
        let mut e = Editor::new("ab\ncd", true, Mode::Insert);
        e.key(Key::Home);
        e.key(Key::WordBackspace);
        assert_eq!((e.text().as_str(), e.cursor()), ("abcd", (0, 2)));
        e.key(Key::Esc);
        assert!(!e.key(Key::WordBackspace));
    }

    #[test]
    fn ciw_and_diw() {
        let mut e = normal("let 값 = foo_bar(1);", false);
        typ(&mut e, "wwwlciwx");
        e.key(Key::Esc);
        assert_eq!((e.text().as_str(), e.cursor()), ("let 값 = x(1);", (0, 8)));
        typ(&mut e, "u");
        assert_eq!(e.text(), "let 값 = foo_bar(1);");
        // 한글 단어, 구두점 덩어리, 공백 덩어리
        typ(&mut e, "0wdiw");
        assert_eq!(
            (e.text().as_str(), e.cursor()),
            ("let  = foo_bar(1);", (0, 4))
        );
        typ(&mut e, "diw");
        assert_eq!(e.text(), "let= foo_bar(1);");
        typ(&mut e, "wwdiw");
        assert_eq!(e.text(), "let= foo_bar1);");
        // ci 다음 다른 키나 Esc 는 대기만 취소, ri 는 그대로 i 로 바꾸기
        typ(&mut e, "0ciq");
        e.key(Key::Esc);
        assert_eq!(
            (e.text().as_str(), e.mode(), e.pending),
            ("let= foo_bar1);", Mode::Normal, None)
        );
        typ(&mut e, "ri");
        assert_eq!(e.text(), "iet= foo_bar1);");
        // 빈 줄에서 ciw 는 Insert 만
        let mut e = normal("", true);
        typ(&mut e, "ciw");
        assert_eq!(e.mode(), Mode::Insert);
    }

    #[test]
    fn hangul_editing() {
        let mut e = normal("가나다", false);
        typ(&mut e, "lx");
        assert_eq!((e.text().as_str(), e.cursor()), ("가다", (0, 1)));
        typ(&mut e, "i라");
        assert_eq!((e.text().as_str(), e.cursor()), ("가라다", (0, 2)));
        e.key(Key::Esc);
        assert_eq!(e.cursor(), (0, 1));
        typ(&mut e, "A마");
        e.key(Key::Backspace);
        e.key(Key::Esc);
        typ(&mut e, "$r하");
        assert_eq!((e.text().as_str(), e.cursor()), ("가라하", (0, 2)));

        let mut e = normal("값 = 한글.길이", false);
        typ(&mut e, "w");
        assert_eq!(e.cursor(), (0, 2));
        typ(&mut e, "wcw영어");
        e.key(Key::Esc);
        assert_eq!(e.text(), "값 = 영어.길이");
        typ(&mut e, "0dw");
        assert_eq!(e.text(), "= 영어.길이");
    }

    #[test]
    fn pending_while_waiting_for_operator_key() {
        let mut e = normal("ab\ncd", true);
        assert!(e.pending.is_none());
        typ(&mut e, "r");
        assert!(e.pending.is_some());
        typ(&mut e, "?");
        assert!(e.pending.is_none());
        assert_eq!(e.text(), "?b\ncd");
        for op in ['y', 'g', 'c', 'd'] {
            e.key(Key::Char(op));
            assert!(e.pending.is_some(), "{op}");
            e.key(Key::Char(op));
            assert!(e.pending.is_none(), "{op}{op}");
            e.key(Key::Esc);
        }
        assert_eq!(e.text(), "cd");
        // Esc 는 대기만 취소한다
        typ(&mut e, "d");
        e.key(Key::Esc);
        assert!(e.pending.is_none());
        assert_eq!(e.text(), "cd");
    }

    #[test]
    fn empty_text_no_panic() {
        let keys = all_keys();
        for multi in [false, true] {
            for mode in [Mode::Normal, Mode::Insert] {
                for &a in &keys {
                    for &b in &keys {
                        let mut e = Editor::new("", multi, mode);
                        e.key(a);
                        check(&e);
                        e.key(b);
                        check(&e);
                    }
                }
            }
        }
    }

    #[test]
    fn random_keys_no_panic() {
        let keys = all_keys();
        let mut seed: u64 = 42;
        for multi in [false, true] {
            let mut e = Editor::new("fn 가a(b: i32) {\n    x.y();\n\n}", multi, Mode::Normal);
            for _ in 0..20_000 {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let before = state(&e);
                if !e.key(keys[(seed >> 33) as usize % keys.len()]) {
                    // false 면 앱이 처리할 키이므로 편집기 상태는 그대로여야 한다
                    assert_eq!(state(&e), before);
                }
                check(&e);
            }
        }
    }
}
