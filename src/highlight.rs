//! Rust 코드 구문 하이라이터 (std 만 사용)
//!
//! 분류는 사용자 nvim 화면(nvim-treesitter rust 쿼리 + onedarkpro Rust 추가 쿼리 +
//! rust-analyzer semantic token)에 맞춘다. 구문 트리 없이 한 번 훑는 렉서라 다음 근사 규칙을 쓴다.
//!
//! - Variable: 다른 범주가 아닌 소문자 식별자 (지역 변수, 매개변수, 패턴 바인딩, 필드 `p.x`, 필드 정의·초기화),
//!   튜플 필드 `p.0`, 포맷 자리표시자 이름 `{x}`·`{n:>4}`, `use std::fmt;` 의 끝 `fmt`, attribute 인자 `dead_code`,
//!   `let [mut] f = [move] |..|` 로 클로저를 묶은 이름의 호출 `f(..)` (메서드 `.f(` 는 Function)
//! - Module: `::` 바로 앞의 소문자 경로(`std::`), `mod` 이름, `super`, 경로·`use` 목록의 `self`
//! - SelfValue: 소문자 `self`. `Self` 는 Type, `crate` 는 Keyword
//! - Function: `fn` 이름과 호출(`f(`, `.m(`, `T::f(`, `f::<T>(`). std 메서드·함수도 Function
//!   (rust-analyzer 는 노랑으로 칠하지만 정의 위치라는 의미 정보가 필요해 treesitter 쪽을 따른다)
//! - Keyword: 키워드와 단독 `_` (와일드카드)
//! - Attribute: attribute 이름(`derive`, `allow`). `#[`·`]`·괄호는 Bracket, 인자는 Type/Variable/Str
//! - Char: 문자 리터럴은 이스케이프(`'\n'`)까지 통째로 Char
//! - Interpolation: 포맷 자리표시자의 `{`, `}`, 서식(`:?`), 위치 인자(`{0}`)
//! - 괄호 `( ) [ ] { }` 는 스택으로 짝을 맞춰 닫는 괄호도 여는 괄호와 같은 Kind 로 칠한다(여러 줄에 걸쳐도).
//!   짝이 안 맞으면 가장 가까운 같은 모양의 여는 괄호까지 닫고, 없으면 Bracket 이다.
//!   - MacroParen: `name!(` 의 `(` 와 그 짝. `vec![`, `m!{` 는 Bracket
//!   - CallParen: 함수·메서드 호출의 인자 괄호와 식 자리의 생성자 `Some(1)`, `Point(1, 2)`
//!   - Bracket: 그 밖의 모든 괄호, 붙여 쓴 제네릭 `<..>`, 클로저 매개변수 `|..|`·`||`
//!   - 근사: 대문자 이름 바로 뒤 `(` 는 패턴 자리(`let`~`=`/`:`, `for`~`in`, match 팔의 `=>` 또는
//!     `if` 가드 전), enum 본문, `struct Name(`, `Fn(`/`FnMut(`/`FnOnce(` 이면 Bracket, 그 밖에는 CallParen.
//!     패턴 자리는 괄호 깊이마다 따로 기억하며 struct 패턴 `Point { x, .. }` 의 `{` 안으로 이어진다.
//!   - 근사: 값(이름·리터럴·닫는 괄호·`_`·`?`) 바로 뒤가 아닌 `|` 는 클로저 매개변수를 열고,
//!     같은 괄호 깊이의 다음 `|` 가 닫는다. 값 뒤의 `|`, `||` 는 연산자
//! - Variant: enum 변형. Some/None/Ok/Err, enum 본문의 변형 이름, 대문자 타입 경로 뒤 `Enum::Variant`
//!   (경로가 더 이어지는 `A::B::c` 의 B 와 ALL_CAPS 는 아니다)
//! - SpecialOperator: rust-analyzer 가 따로 칠하는 연산자 중 값 뒤의 `?` 와 복합 대입(`+=`, `<<=`).
//!   rust-analyzer 는 매크로 인자 안의 연산자(`.`, `::`, `&`, `+`)와 음수 `-1` 도 따로 칠하지만, 같은 기호가
//!   매크로 안팎에서 색이 달라지는 확장 부산물이라 의도적으로 따르지 않고 Operator 로 둔다

use crate::BLANK;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plain,
    Keyword,
    Type,
    Function,
    Macro,
    Constant,
    Str,
    Char,
    Escape,
    Interpolation,
    Number,
    Comment,
    Lifetime,
    Attribute,
    Operator,
    Variable,
    Module,
    SelfValue,
    Bracket,
    CallParen,
    MacroParen,
    Variant,
    SpecialOperator,
    Blank,
}

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "static", "struct", "super", "trait", "type", "unsafe", "use", "where", "while",
];

const PRIMITIVES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str",
];

/// 줄을 넘어 이어지는 문자열·주석 상태
#[derive(Clone, Copy)]
enum State {
    Code,
    LineComment,
    /// 블록 주석 중첩 깊이
    Block(usize),
    /// raw 이면 `#` 개수, fmt 면 포맷 자리표시자(`{x}`)를 강조
    Str {
        raw: Option<usize>,
        fmt: bool,
    },
    Attr(Attr),
}

/// attribute `#[..]` 안
#[derive(Clone, Copy)]
struct Attr {
    /// `[` 깊이
    depth: usize,
    /// attribute 안 문자열
    in_str: bool,
    /// attribute 이름(`derive`)을 지났다
    named: bool,
}

/// 다음 `{` 나 `(` 의 뜻을 정하는 키워드
#[derive(Clone, Copy, PartialEq, Eq)]
enum Opener {
    Match,
    Enum,
    Struct,
}

/// 문장 하나 동안 이어지는 문맥. `;` 에서 지운다
#[derive(Clone, Copy, Default)]
struct StmtCtx {
    /// 지금 패턴 자리다 (`let`~`=`, `for`~`in`, match 팔의 `=>` 전)
    pattern: bool,
    /// 다음 `{`/`(` 의 뜻을 정한 키워드
    opener: Option<Opener>,
    /// `use` 문 안
    use_decl: bool,
    /// `impl` 헤더 안 (`impl A for B` 의 for 는 루프가 아니다)
    impl_header: bool,
}

/// 괄호 스택 한 칸. 괄호 밖 최상위는 `Lexer::base` 가 맡는다.
#[derive(Clone, Copy)]
struct Frame {
    open: char,
    kind: Kind,
    /// 이 `{` 를 연 키워드. match 본문은 `,` 와 팔 블록 `}` 뒤에 새 팔 패턴이 시작되고,
    /// enum 본문의 변형 정의 `A(i32)` 는 생성자 호출이 아니다
    body: Option<Opener>,
    stmt: StmtCtx,
}

/// 이 줄에서 내보낸 공백 아닌 토큰 하나 (병합 전 단위, 주석 포함). start..end 는 줄 안 글자 위치
#[derive(Clone, Copy)]
struct Tok {
    kind: Kind,
    start: usize,
    end: usize,
}

/// code.lines() 와 같은 줄 단위로 토큰을 돌려준다. 각 줄의 토큰 text 를 이으면 원래 줄과 정확히 같다(무손실).
pub fn highlight(code: &str) -> Vec<Vec<(Kind, String)>> {
    highlight_lines(code.lines())
}

/// 줄마다 토큰을 돌려준다. 출력 줄 수는 입력 줄 수와 같고, 문자열·주석·괄호 문맥은 다음 줄로 이어진다.
pub fn highlight_lines<'a>(lines: impl IntoIterator<Item = &'a str>) -> Vec<Vec<(Kind, String)>> {
    let mut lexer = Lexer::new();
    lines.into_iter().map(|line| lexer.line(line)).collect()
}

fn starts_with(chars: &[char], i: usize, s: &str) -> bool {
    let mut rest = chars.iter().skip(i);
    s.chars().all(|c| rest.next() == Some(&c))
}

fn is_ident(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn is_op(c: char) -> bool {
    "=+-*/%!&|^<>?:.@~".contains(c)
}

/// 2글자 이상 ALL_CAPS (MAX_SIZE, PI)
fn is_constant(name: &str) -> bool {
    name.len() >= 2
        && name.starts_with(|c: char| c.is_ascii_uppercase())
        && !name.contains(|c: char| c.is_ascii_lowercase())
}

/// i 가 줄 안이고 빈칸 시작이 아니다
fn usable(chars: &[char], i: usize) -> bool {
    i < chars.len() && !starts_with(chars, i, BLANK)
}

/// i 부터 식별자 문자 길이 (빈칸 앞에서 멈춘다)
fn ident_len(chars: &[char], i: usize) -> usize {
    let mut end = i;
    while chars.get(end).is_some_and(|&c| is_ident(c)) && !starts_with(chars, end, BLANK) {
        end += 1;
    }
    end - i
}

/// i 의 `\` 로 시작하는 이스케이프 길이 (`\n`, `\x41`, `\u{AC00}`)
fn escape_len(chars: &[char], i: usize) -> usize {
    if !usable(chars, i + 1) {
        return 1;
    }
    let hex = |j: usize| chars.get(j).is_some_and(|c| c.is_ascii_hexdigit());
    match chars[i + 1] {
        'x' => 2 + (i + 2..i + 4).take_while(|&j| hex(j)).count(),
        'u' if chars.get(i + 2) == Some(&'{') => {
            let n = (i + 3..).take_while(|&j| hex(j)).count();
            3 + n + usize::from(chars.get(i + 3 + n) == Some(&'}'))
        }
        _ => 2,
    }
}

/// kind 토큰 text 가 값(피연산자)으로 끝난다. 아니면 다음 `|` 는 클로저 매개변수를 연다
fn ends_operand(kind: Kind, text: &[char]) -> bool {
    let last = text.iter().rev().find(|c| !c.is_whitespace());
    match kind {
        Kind::Bracket | Kind::CallParen | Kind::MacroParen => matches!(last, Some(')' | ']' | '}')),
        Kind::Keyword => text == ['_'],
        Kind::Operator | Kind::SpecialOperator => last == Some(&'?'),
        Kind::Plain | Kind::Macro | Kind::Attribute | Kind::Blank => false,
        _ => true,
    }
}

struct Lexer {
    /// 줄을 넘어 이어지는 문자열·주석 상태
    state: State,
    // chars ~ call_paren_at 은 줄마다 새로 시작한다 (`line` 참고). 그 아래는 줄을 넘어 이어진다
    chars: Vec<char>,
    pos: usize,
    tokens: Vec<(Kind, String)>,
    /// 이 줄에서 내보낸 공백 아닌 토큰. 뒤를 보는 판정은 렌더용으로 병합한 tokens 가 아니라 이것을 본다
    seen: Vec<Tok>,
    /// 공백만 지나온 직전 토큰이 이 키워드 (`fn`, `mod`)
    after_kw: Option<&'static str>,
    /// 열린 제네릭 `<` 개수
    generics: usize,
    /// 이 위치 전까지는 포맷 자리표시자
    interp_end: usize,
    /// 이 위치의 `(` 는 호출 인자 괄호
    call_paren_at: Option<usize>,
    /// 괄호 밖 최상위 문맥
    base: Frame,
    /// 열린 괄호들 (줄을 넘어 이어진다)
    stack: Vec<Frame>,
    /// 마지막으로 내보낸 공백·주석 아닌 토큰의 Kind 와 그것이 값으로 끝나는지 (줄을 넘어 이어진다)
    last: Option<(Kind, bool)>,
    /// 클로저 매개변수 `|..|` 가 열린 괄호 깊이
    closure: Option<usize>,
    /// `let name = |..|` 로 클로저를 묶은 이름들 (호출해도 변수)
    closures: Vec<String>,
}

impl Lexer {
    fn new() -> Self {
        Lexer {
            state: State::Code,
            chars: Vec::new(),
            pos: 0,
            tokens: Vec::new(),
            seen: Vec::new(),
            after_kw: None,
            generics: 0,
            interp_end: 0,
            call_paren_at: None,
            base: Frame {
                open: ' ',
                kind: Kind::Bracket,
                body: None,
                stmt: StmtCtx::default(),
            },
            stack: Vec::new(),
            last: None,
            closure: None,
            closures: Vec::new(),
        }
    }

    /// 한 줄의 토큰. 줄 안에서만 쓰는 상태는 여기서 새로 시작한다
    fn line(&mut self, line: &str) -> Vec<(Kind, String)> {
        self.chars.clear();
        self.chars.extend(line.chars());
        self.pos = 0;
        self.seen.clear();
        self.after_kw = None;
        self.generics = 0;
        self.interp_end = 0;
        self.call_paren_at = None;
        while let Some(c) = self.peek(0) {
            if self.at(BLANK) {
                self.take(Kind::Blank, BLANK.len());
                continue;
            }
            self.state = match self.state {
                State::Code => self.code(c),
                State::LineComment => {
                    self.take(Kind::Comment, 1);
                    State::LineComment
                }
                State::Block(depth) => self.block(depth),
                State::Str { raw, fmt } => self.string(c, raw, fmt),
                State::Attr(attr) => self.attr(c, attr),
            };
        }
        if let State::LineComment = self.state {
            self.state = State::Code;
        }
        std::mem::take(&mut self.tokens)
    }

    fn at(&self, s: &str) -> bool {
        starts_with(&self.chars, self.pos, s)
    }

    fn peek(&self, k: usize) -> Option<char> {
        self.chars.get(self.pos + k).copied()
    }

    /// 직전 글자
    fn prev_char(&self) -> Option<char> {
        self.pos.checked_sub(1).map(|i| self.chars[i])
    }

    /// 직전 토큰이 값(피연산자)으로 끝난다
    fn prev_operand(&self) -> bool {
        self.last.is_some_and(|(_, operand)| operand)
    }

    fn top(&self) -> Frame {
        self.stack.last().copied().unwrap_or(self.base)
    }

    fn top_mut(&mut self) -> &mut Frame {
        self.stack.last_mut().unwrap_or(&mut self.base)
    }

    /// len 글자를 kind 로 내보낸다. 같은 kind 가 이어지면 합친다(Blank 제외).
    fn take(&mut self, kind: Kind, len: usize) {
        let start = self.pos;
        self.pos = (start + len).min(self.chars.len());
        let text = &self.chars[start..self.pos];
        if !text.iter().all(|c| c.is_whitespace()) {
            self.after_kw = None;
            self.seen.push(Tok {
                kind,
                start,
                end: self.pos,
            });
            if kind != Kind::Comment {
                self.last = Some((kind, ends_operand(kind, text)));
            }
        }
        match self.tokens.last_mut() {
            Some((last, s)) if *last == kind && kind != Kind::Blank => s.extend(text),
            _ => self.tokens.push((kind, text.iter().collect())),
        }
    }

    fn code(&mut self, c: char) -> State {
        match c {
            '/' if self.peek(1) == Some('/') => {
                self.take(Kind::Comment, 2);
                State::LineComment
            }
            '/' if self.peek(1) == Some('*') => {
                self.take(Kind::Comment, 2);
                State::Block(1)
            }
            '#' if self.at("#[") || self.at("#![") => {
                let len = if self.peek(1) == Some('!') { 3 } else { 2 };
                self.take(Kind::Bracket, len);
                State::Attr(Attr {
                    depth: 1,
                    in_str: false,
                    named: false,
                })
            }
            '"' => {
                self.take(Kind::Str, 1);
                State::Str {
                    raw: None,
                    fmt: true,
                }
            }
            '\'' => {
                self.quote();
                State::Code
            }
            '0'..='9' => {
                self.number();
                State::Code
            }
            '(' | '[' | '{' => {
                let call = c == '(' && self.call_paren_at == Some(self.pos);
                self.open(c, if call { Kind::CallParen } else { Kind::Bracket });
                State::Code
            }
            ')' | ']' | '}' => {
                self.close(c);
                State::Code
            }
            ';' => {
                self.closure = None;
                self.top_mut().stmt = StmtCtx::default();
                self.take(Kind::Plain, 1);
                State::Code
            }
            ',' => {
                let top = self.top_mut();
                if top.body == Some(Opener::Match) {
                    top.stmt.pattern = true;
                }
                self.take(Kind::Plain, 1);
                State::Code
            }
            c if is_ident(c) => self.word(),
            c if is_op(c) => {
                self.operator(c);
                State::Code
            }
            _ => {
                self.take(Kind::Plain, 1);
                State::Code
            }
        }
    }

    /// 여는 괄호 c 를 kind 로 내보내고 스택에 올린다
    fn open(&mut self, c: char, kind: Kind) {
        let parent = self.top().stmt;
        let mut frame = Frame {
            open: c,
            kind,
            body: None,
            stmt: StmtCtx {
                pattern: parent.pattern,
                use_decl: parent.use_decl,
                ..StmtCtx::default()
            },
        };
        if c == '{' {
            frame.body = parent.opener;
            // struct 패턴 `Point { x, y }` 만 패턴을 이어 가고, match 본문은 첫 팔 패턴으로 시작한다
            frame.stmt.pattern = (parent.pattern
                && matches!(self.last, Some((Kind::Type | Kind::Variant, _))))
                || frame.body == Some(Opener::Match);
            let top = &mut self.top_mut().stmt;
            top.opener = None;
            top.impl_header = false;
        }
        self.take(kind, 1);
        self.stack.push(frame);
    }

    fn close(&mut self, c: char) {
        let open = match c {
            ')' => '(',
            ']' => '[',
            _ => '{',
        };
        let kind = match self.stack.iter().rposition(|f| f.open == open) {
            Some(i) => {
                let kind = self.stack[i].kind;
                self.stack.truncate(i);
                kind
            }
            None => Kind::Bracket,
        };
        // 블록 팔 `X => { .. }` 뒤에는 쉼표 없이 다음 팔 패턴이 온다
        let top = self.top_mut();
        if c == '}' && top.body == Some(Opener::Match) {
            top.stmt.pattern = true;
        }
        self.take(kind, 1);
    }

    fn operator(&mut self, c: char) {
        let next = self.peek(1);
        match c {
            '-' | '=' if next == Some('>') => {
                if c == '=' {
                    self.top_mut().stmt.pattern = false;
                }
                self.take(Kind::Operator, 2)
            }
            // 복합 대입 `+=`, `<<=` 와 값 뒤의 `?` 는 rust-analyzer 가 따로 칠한다
            '+' | '-' | '*' | '/' | '%' | '^' | '&' | '|' if next == Some('=') => {
                self.take(Kind::SpecialOperator, 2)
            }
            '<' | '>'
                if next == Some(c)
                    && self.peek(2) == Some('=')
                    && (c == '<' || self.generics == 0) =>
            {
                self.take(Kind::SpecialOperator, 3)
            }
            '?' if self.prev_operand() => self.take(Kind::SpecialOperator, 1),
            '<' if self.opens_generic() => {
                self.generics += 1;
                self.take(Kind::Bracket, 1);
            }
            '>' if self.generics > 0 => {
                self.generics -= 1;
                self.take(Kind::Bracket, 1);
            }
            '|' if self.closure == Some(self.stack.len()) => {
                self.closure = None;
                self.take(Kind::Bracket, 1);
            }
            // 값 뒤가 아닌 `|` 는 클로저 매개변수를 연다. `||` 는 매개변수 없는 클로저
            '|' if !self.prev_operand() => {
                self.note_closure_binding();
                if next == Some('|') {
                    self.take(Kind::Bracket, 2);
                } else {
                    self.closure = Some(self.stack.len());
                    self.take(Kind::Bracket, 1);
                }
            }
            // 두 번째 `|` 가 클로저로 보이지 않게 `||` 는 한 번에
            '|' if next == Some('|') => self.take(Kind::Operator, 2),
            // 이름이 빈칸인 매크로: `___!(`
            '!' if matches!(next, Some('(' | '[' | '{'))
                && self
                    .seen
                    .last()
                    .is_some_and(|t| t.kind == Kind::Blank && t.end == self.pos) =>
            {
                self.macro_name(1)
            }
            _ => {
                // 패턴은 `=`(값) 나 단독 `:`(타입 표기) 에서 끝난다. `::` 는 경로라 이어진다
                if c == '=' || (c == ':' && next != Some(':') && self.prev_char() != Some(':')) {
                    self.top_mut().stmt.pattern = false;
                }
                self.take(Kind::Operator, 1)
            }
        }
    }

    /// `name!` (len 글자)을 Macro 로 내보내고, 바로 뒤 `(` 는 매크로 괄호로 연다
    fn macro_name(&mut self, len: usize) {
        self.take(Kind::Macro, len);
        if self.peek(0) == Some('(') {
            self.open('(', Kind::MacroParen);
        }
    }

    /// 지금 여는 클로저가 `let [mut] name = [move]` 의 값이면 name 을 기억한다
    fn note_closure_binding(&mut self) {
        let chars = &self.chars;
        let mut seen = self
            .seen
            .iter()
            .rev()
            .map(|t| (t.kind, &chars[t.start..t.end]));
        let is = |t: Option<(Kind, &[char])>, kind: Kind, text: &str| {
            t.is_some_and(|(k, s)| k == kind && s.iter().copied().eq(text.chars()))
        };
        let mut t = seen.next();
        if is(t, Kind::Keyword, "move") {
            t = seen.next();
        }
        if !is(t, Kind::Operator, "=") {
            return;
        }
        let Some((Kind::Variable, name)) = seen.next() else {
            return;
        };
        let name: String = name.iter().collect();
        let mut t = seen.next();
        if is(t, Kind::Keyword, "mut") {
            t = seen.next();
        }
        if is(t, Kind::Keyword, "let") {
            self.closures.push(name);
        }
    }

    /// `Vec<`, `impl<`, `::<` 처럼 붙여 쓴 `<` 는 비교 연산자가 아니라 제네릭 괄호
    fn opens_generic(&self) -> bool {
        !matches!(self.peek(1), Some('=' | '<'))
            && self.prev_char().is_some_and(|p| is_ident(p) || p == ':')
    }

    fn block(&mut self, depth: usize) -> State {
        if self.at("/*") {
            self.take(Kind::Comment, 2);
            return State::Block(depth + 1);
        }
        if self.at("*/") {
            self.take(Kind::Comment, 2);
            return if depth > 1 {
                State::Block(depth - 1)
            } else {
                State::Code
            };
        }
        self.take(Kind::Comment, 1);
        State::Block(depth)
    }

    fn string(&mut self, c: char, raw: Option<usize>, fmt: bool) -> State {
        if self.pos < self.interp_end {
            // `{name:?}` 의 name 은 변수
            let name = (self.pos..self.interp_end)
                .take_while(|&i| {
                    let c = self.chars[i];
                    (c.is_alphanumeric() || c == '_') && !starts_with(&self.chars, i, BLANK)
                })
                .count();
            if self.prev_char() == Some('{') && !c.is_ascii_digit() && name > 0 {
                self.take(Kind::Variable, name);
            } else {
                self.take(Kind::Interpolation, 1);
            }
            return State::Str { raw, fmt };
        }
        match (c, raw) {
            ('"', None) => {
                self.take(Kind::Str, 1);
                return State::Code;
            }
            ('"', Some(h)) if (1..=h).all(|k| self.peek(k) == Some('#')) => {
                self.take(Kind::Str, 1 + h);
                return State::Code;
            }
            ('\\', None) => {
                let len = escape_len(&self.chars, self.pos);
                self.take(Kind::Escape, len);
            }
            ('{', None) if fmt => self.placeholder(),
            _ => self.take(Kind::Str, 1),
        }
        State::Str { raw, fmt }
    }

    /// `{}`, `{x:?}`, `{0:>8}` 같은 포맷 자리표시자. `{{` 는 그냥 문자열
    fn placeholder(&mut self) {
        if self.peek(1) == Some('{') {
            self.take(Kind::Str, 2);
            return;
        }
        let close = self.chars[self.pos + 1..]
            .iter()
            .position(|c| matches!(c, '{' | '}' | '"' | '\\'))
            .map(|i| self.pos + 1 + i);
        match close {
            Some(i) if self.chars[i] == '}' => {
                self.interp_end = i + 1;
                self.take(Kind::Interpolation, 1);
            }
            _ => self.take(Kind::Str, 1),
        }
    }

    /// attribute 안: `#[`, `]`, 괄호는 Bracket, 첫 이름은 Attribute, 인자는 Type/Variable/Str
    fn attr(&mut self, c: char, mut attr: Attr) -> State {
        if attr.in_str {
            // 포맷 자리표시자 없는 보통 문자열과 같다
            attr.in_str = matches!(self.string(c, None, false), State::Str { .. });
            return State::Attr(attr);
        }
        match c {
            '"' => {
                self.take(Kind::Str, 1);
                attr.in_str = true;
            }
            '[' => {
                self.take(Kind::Bracket, 1);
                attr.depth += 1;
            }
            ']' => {
                self.take(Kind::Bracket, 1);
                attr.depth -= 1;
                if attr.depth == 0 {
                    return State::Code;
                }
            }
            '(' | ')' | '{' | '}' => self.take(Kind::Bracket, 1),
            '0'..='9' => self.number(),
            c if is_ident(c) => {
                let len = ident_len(&self.chars, self.pos).max(1);
                let name: String = self.chars[self.pos..self.pos + len].iter().collect();
                let kind = if !attr.named {
                    Kind::Attribute
                } else if PRIMITIVES.contains(&name.as_str())
                    || name.starts_with(|c: char| c.is_ascii_uppercase())
                {
                    Kind::Type
                } else if matches!(name.as_str(), "true" | "false") {
                    Kind::Constant
                } else {
                    Kind::Variable
                };
                self.take(kind, len);
                attr.named = true;
            }
            c if is_op(c) => self.take(Kind::Operator, 1),
            _ => self.take(Kind::Plain, 1),
        }
        State::Attr(attr)
    }

    /// pos+off 의 `'` 로 시작하는 문자 리터럴(`'a'`, `'\n'`, `'\u{41}'`) 길이
    fn char_len(&self, off: usize) -> Option<usize> {
        let q = self.pos + off;
        let chars = &self.chars;
        if !usable(chars, q + 1) {
            return None;
        }
        let close = if chars[q + 1] == '\\' {
            if !usable(chars, q + 2) {
                return None;
            }
            (q + 3..chars.len())
                .take_while(|&i| usable(chars, i))
                .find(|&i| chars[i] == '\'')?
        } else {
            q + 2
        };
        (chars.get(close) == Some(&'\'')).then_some(close + 1 - q)
    }

    fn quote(&mut self) {
        // 문자 리터럴은 이스케이프(`'\\n'`)까지 통째로 Char (nvim @character)
        if let Some(len) = self.char_len(0) {
            self.take(Kind::Char, len);
            return;
        }
        let n = ident_len(&self.chars, self.pos + 1);
        if n > 0 && !self.peek(1).is_some_and(|c| c.is_ascii_digit()) {
            self.take(Kind::Lifetime, 1 + n);
        } else {
            self.take(Kind::Plain, 1);
        }
    }

    fn number(&mut self) {
        let chars = &self.chars;
        let start = self.pos;
        // 접미사(u32, f64 ...)와 0x/0b/0o 는 식별자 문자로 함께 읽는다
        let mut len = ident_len(chars, start);
        // 소수점은 뒤가 숫자일 때만 (1..5 는 범위, x.0.1 은 튜플 인덱스)
        let before = &chars[..start];
        let tuple_index = before.ends_with(&['.']) && !before.ends_with(&['.', '.']);
        if !tuple_index
            && self.peek(len) == Some('.')
            && self.peek(len + 1).is_some_and(|c| c.is_ascii_digit())
        {
            len += 1 + ident_len(chars, start + len + 1);
        }
        // 지수 부호: 1e-3, 2.5E+10
        let (last, mantissa) = chars[start..start + len]
            .split_last()
            .unwrap_or((&' ', &[]));
        if matches!(last, 'e' | 'E')
            && mantissa
                .iter()
                .all(|c| c.is_ascii_digit() || matches!(c, '_' | '.'))
            && matches!(self.peek(len), Some('+' | '-'))
            && self.peek(len + 1).is_some_and(|c| c.is_ascii_digit())
        {
            len += 1 + ident_len(chars, start + len + 1);
        }
        // 튜플 필드 `point.0` 은 필드 (rust-analyzer property)
        let kind = if tuple_index {
            Kind::Variable
        } else {
            Kind::Number
        };
        self.take(kind, len);
    }

    /// 이름 바로 뒤에 `(` 또는 `::<..>(` 가 오면 그 `(` 위치
    fn call_paren(&self, len: usize) -> Option<usize> {
        let mut i = self.pos + len;
        if starts_with(&self.chars, i, "::<") {
            let mut depth = 0;
            for (j, &c) in self.chars.iter().enumerate().skip(i + 2) {
                match c {
                    '<' => depth += 1,
                    '>' => depth -= 1,
                    _ => {}
                }
                if depth == 0 {
                    i = j + 1;
                    break;
                }
            }
        }
        (self.chars.get(i) == Some(&'(')).then_some(i)
    }

    /// 대문자 이름 뒤 `(` 가 식 자리의 생성자 호출인가 (패턴·타입·정의 자리면 아니다)
    fn constructs(&self, name: &str) -> bool {
        let top = self.top();
        !matches!(name, "Fn" | "FnMut" | "FnOnce")
            && !top.stmt.pattern
            && top.body != Some(Opener::Enum)
            && top.stmt.opener != Some(Opener::Struct)
    }

    /// enum 변형: Some/None/Ok/Err, enum 본문의 변형 이름, 대문자 타입 경로 뒤 `Enum::Variant`.
    /// 경로가 더 이어지면(`path`, `A::B::c` 의 B) 아니다
    fn is_variant(&self, name: &str, path: bool) -> bool {
        !path
            && (matches!(name, "Some" | "None" | "Ok" | "Err")
                || self.top().body == Some(Opener::Enum)
                || self.after_type_path())
    }

    /// 바로 앞이 공백 없이 붙여 쓴 대문자 타입 경로 `Enum::` 다
    fn after_type_path(&self) -> bool {
        let [.., ty, a, b] = self.seen[..] else {
            return false;
        };
        matches!(ty.kind, Kind::Type | Kind::Variant)
            && ty.end == a.start
            && a.end == b.start
            && b.end == self.pos
            && self.chars[a.start..b.end] == [':', ':']
    }

    fn word(&mut self) -> State {
        // 문자열 접두사: b"..", b'..', r"..", r#".."#, br".."
        let prefix = if self.at("br") {
            2
        } else if self.at("b") || self.at("r") {
            1
        } else {
            0
        };
        if prefix > 0 {
            let raw = self.peek(prefix - 1) == Some('r');
            let mut hashes = 0;
            while raw && self.peek(prefix + hashes) == Some('#') {
                hashes += 1;
            }
            if self.peek(prefix + hashes) == Some('"') {
                self.take(Kind::Str, prefix + hashes + 1);
                return State::Str {
                    raw: raw.then_some(hashes),
                    fmt: false,
                };
            }
            if !raw
                && self.peek(1) == Some('\'')
                && let Some(len) = self.char_len(1)
            {
                self.take(Kind::Char, 1 + len);
                return State::Code;
            }
        }

        // raw 식별자 r#type 은 키워드로 보지 않는다
        let skip = if self.at("r#")
            && !self.peek(2).is_some_and(|c| c.is_ascii_digit())
            && ident_len(&self.chars, self.pos + 2) > 0
        {
            2
        } else {
            0
        };
        let len = skip + ident_len(&self.chars, self.pos + skip);
        if self.peek(len) == Some('!') && self.peek(len + 1) != Some('=') {
            self.macro_name(len + 1);
            return State::Code;
        }
        let name: String = self.chars[self.pos + skip..self.pos + len].iter().collect();
        let kind = self.classify(&name, skip > 0, len);
        self.take(kind, len);
        if kind == Kind::Keyword {
            self.keyword(&name);
        }
        State::Code
    }

    /// pos 에서 len 글자인 식별자 name 의 Kind. 호출이면 인자 괄호 위치도 기억한다.
    fn classify(&mut self, name: &str, raw: bool, len: usize) -> Kind {
        let path = starts_with(&self.chars, self.pos + len, "::");
        let top = self.top();
        if !raw {
            match name {
                "self" if top.stmt.use_decl || path => return Kind::Module,
                "self" => return Kind::SelfValue,
                "super" => return Kind::Module,
                "true" | "false" => return Kind::Constant,
                "_" => return Kind::Keyword,
                _ if KEYWORDS.contains(&name) => return Kind::Keyword,
                _ => {}
            }
        }
        match self.after_kw {
            Some("fn") => return Kind::Function,
            Some("mod") => return Kind::Module,
            _ => {}
        }
        if PRIMITIVES.contains(&name) {
            return Kind::Type;
        }
        if name.starts_with(|c: char| c.is_ascii_uppercase()) {
            if self.peek(len) == Some('(') && self.constructs(name) {
                self.call_paren_at = Some(self.pos + len);
            }
            return if is_constant(name) {
                Kind::Constant
            } else if self.is_variant(name, path) {
                Kind::Variant
            } else {
                Kind::Type
            };
        }
        if let Some(paren) = self.call_paren(len) {
            self.call_paren_at = Some(paren);
            // 클로저를 묶은 변수 호출 `add_one(5)` 은 변수 (메서드 `.add_one(` 은 아니다)
            let method = self.prev_char() == Some('.');
            return if !method && self.closures.iter().any(|c| c == name) {
                Kind::Variable
            } else {
                Kind::Function
            };
        }
        if path {
            return Kind::Module;
        }
        Kind::Variable
    }

    /// 키워드가 뒤따르는 문맥에 남기는 표시
    fn keyword(&mut self, name: &str) {
        let hrtb = self.chars[self.pos..]
            .iter()
            .find(|c| !c.is_whitespace())
            .is_some_and(|&c| c == '<');
        let stmt = &mut self.top_mut().stmt;
        match name {
            "fn" => self.after_kw = Some("fn"),
            "mod" => self.after_kw = Some("mod"),
            "let" => stmt.pattern = true,
            "for" if !stmt.impl_header && !hrtb => stmt.pattern = true,
            "in" | "if" => stmt.pattern = false,
            "match" => stmt.opener = Some(Opener::Match),
            "enum" => stmt.opener = Some(Opener::Enum),
            "struct" => stmt.opener = Some(Opener::Struct),
            "use" => stmt.use_decl = true,
            "impl" => stmt.impl_header = true,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Kind::*;
    use super::*;

    fn check(code: &str, expected: &[&[(Kind, &str)]]) {
        let want: Vec<Vec<(Kind, String)>> = expected
            .iter()
            .map(|line| line.iter().map(|&(k, s)| (k, s.to_string())).collect())
            .collect();
        assert_eq!(highlight(code), want, "{code}");
    }

    fn check1(code: &str, expected: &[(Kind, &str)]) {
        check(code, &[expected]);
    }

    /// code 안의 각 (text, kind) 가 그 순서대로 그 Kind 토큰으로 나온다 (공백 토큰은 건너뛴다)
    fn kinds(code: &str, expected: &[(&str, Kind)]) {
        let lines = highlight(code);
        let tokens: Vec<(Kind, String)> = lines
            .into_iter()
            .flatten()
            .filter(|(_, s)| !s.trim().is_empty())
            .collect();
        let mut rest = tokens.iter();
        for &(text, kind) in expected {
            let found = rest.find(|(_, s)| s.trim() == text);
            assert_eq!(
                found.map(|(k, _)| *k),
                Some(kind),
                "{code}\n{text:?} 의 Kind (전체: {tokens:?})"
            );
        }
    }

    fn assert_lossless(code: &str) {
        let lines = highlight(code);
        assert_eq!(lines.len(), code.lines().count(), "{code:?}");
        for (line, tokens) in code.lines().zip(&lines) {
            let joined: String = tokens.iter().map(|(_, s)| s.as_str()).collect();
            assert_eq!(joined, line, "{code:?}");
            // 인접한 같은 Kind 는 합쳐져 있다 (Blank 제외)
            for pair in tokens.windows(2) {
                assert!(
                    pair[0].0 != pair[1].0 || pair[0].0 == Blank,
                    "{code:?}: {tokens:?}"
                );
            }
        }
    }

    #[test]
    fn highlight_lines_keeps_line_count() {
        // 빈 줄, 블록 주석 안 빈 줄, 끝 빈 줄도 한 줄씩 나온다
        let lines = ["fn f() {", "", "    /* a", "", "*/ }", "", ""];
        let out = highlight_lines(lines);
        assert_eq!(out.len(), lines.len());
        for i in [1, 3, 5, 6] {
            assert!(out[i].is_empty(), "{i}: {:?}", out[i]);
        }
        assert_eq!(out[4][0], (Comment, "*/".to_string()));
        assert_eq!(out[4].last(), Some(&(Bracket, "}".to_string())));

        // highlight 는 code.lines() 를 따르고(끝 개행 뒤 빈 줄 없음), 같은 줄이면 결과도 같다
        let code = "let s = \"a\n\nb\";\n";
        assert_eq!(highlight(code).len(), 3);
        assert_eq!(highlight(code), highlight_lines(code.lines()));
        assert_eq!(highlight_lines(code.split('\n')).len(), 4);
        assert!(highlight_lines([]).is_empty());
    }

    #[test]
    fn lessons_are_lossless_and_blanks_match_answers() {
        let lessons = crate::load_lessons(std::path::Path::new(crate::LESSONS_DIR)).unwrap();
        for lesson in &lessons {
            assert_lossless(&lesson.example);
            for quiz in &lesson.quiz {
                assert_lossless(&quiz.code);
                let blanks = highlight(&quiz.code)
                    .iter()
                    .flatten()
                    .filter(|(k, _)| *k == Blank)
                    .count();
                assert_eq!(blanks, quiz.answers.len(), "{}: {}", lesson.id, quiz.code);
            }
        }
    }

    #[test]
    fn broken_input_is_lossless() {
        for code in [
            "\"abc",
            "'",
            "'\\",
            "'\\u{41",
            "'\\u{___}'",
            "b'",
            "b'\\",
            "r#\"x",
            "r#",
            "r#___",
            "r#1",
            "br",
            "/*",
            "/* /* */",
            "#[",
            "#[a\n\"b",
            "#[doc = \"\\",
            "#!",
            "0x",
            "1.",
            "1e",
            "1e-",
            "1e+___",
            "\"\\",
            "\"\\u{",
            "\"\\u{4",
            "\"\\x",
            "\"\\x4",
            "\"{",
            "\"{x",
            "\"{___",
            "\"{{",
            "\"}",
            "f::<",
            "f::<i32",
            "f::<<>",
            "f::<>>(",
            "x>>=y<<=z",
            "Vec<",
            ">",
            "-",
            "_",
            "__",
            "____",
            "_____",
            "'___'",
            "'\\___'",
            "\"\\___\"",
            "a\n\"b\nc",
            "x!",
            "!",
            "___!",
            "___!(",
            "x!(",
            "fn",
            "fn ",
            "mod",
            "use",
            "use std::{",
            "let",
            "let Some(",
            "match",
            "match x {",
            "match x { Some(",
            "enum E { A(",
            "struct S(",
            "impl A for",
            "for<",
            "self::",
            "가나 'ㄱ' \"다\" // 라",
            "\n\n",
        ] {
            assert_lossless(code);
        }
    }

    #[test]
    fn unbalanced_brackets_are_lossless() {
        for code in [
            ")",
            "]",
            "}",
            "(",
            "[",
            "{",
            ")(",
            "}{",
            "(]",
            "[)",
            "{)",
            "(}",
            "f(]",
            "f(}",
            "f(\n]\n)",
            "x!(]",
            "x!(}",
            "x!{)",
            "x![)",
            "Some(}",
            "((([",
            ")))]]]}}}",
            "({[)]}",
            "f({)}",
            "match x { A(",
        ] {
            assert_lossless(code);
        }
        // 짝이 없는 닫는 괄호는 Bracket, 모양이 다르면 가장 가까운 같은 모양까지 닫는다
        check1(
            "f(])",
            &[
                (Function, "f"),
                (CallParen, "("),
                (Bracket, "]"),
                (CallParen, ")"),
            ],
        );
        check1(
            "f({)}",
            &[
                (Function, "f"),
                (CallParen, "("),
                (Bracket, "{"),
                (CallParen, ")"),
                (Bracket, "}"),
            ],
        );
    }

    /// 괄호·문자열·빈칸·키워드 조각을 섞은 입력에서도 패닉 없이 무손실
    #[test]
    fn random_fragments_are_lossless() {
        const PARTS: &[&str] = &[
            "(", ")", "[", "]", "{", "}", "___", "'", "\"", "r#", "#[", "/*", "*/", "//", "\\",
            "{x}", "foo", "Some", "MAX", "self", "Self", "_", "!", "::", "::<", "<", ">", "\n",
            " ", "let", "match", "for", "in", "if", "enum", "struct", "use", "mod", "fn", "impl",
            "=>", ",", ";", "=", ":", ".", "|", "1", "가", "b", "x!", "'a",
        ];
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        for _ in 0..3000 {
            let mut code = String::new();
            for _ in 0..12 {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                code.push_str(PARTS[(seed % PARTS.len() as u64) as usize]);
            }
            assert_lossless(&code);
        }
    }

    #[test]
    fn blank_in_string() {
        check1(
            r#"    ___!("안녕, {___}!");"#,
            &[
                (Plain, "    "),
                (Blank, "___"),
                (Macro, "!"),
                (MacroParen, "("),
                (Str, "\"안녕, "),
                (Interpolation, "{"),
                (Blank, "___"),
                (Interpolation, "}"),
                (Str, "!\""),
                (MacroParen, ")"),
                (Plain, ";"),
            ],
        );
        check1(
            r#"println!("{} 와 {___}", a);"#,
            &[
                (Macro, "println!"),
                (MacroParen, "("),
                (Str, "\""),
                (Interpolation, "{}"),
                (Str, " 와 "),
                (Interpolation, "{"),
                (Blank, "___"),
                (Interpolation, "}"),
                (Str, "\""),
                (Plain, ", "),
                (Variable, "a"),
                (MacroParen, ")"),
                (Plain, ";"),
            ],
        );
    }

    #[test]
    fn blank_splits_number_and_ident() {
        check1(
            "for i in 1___5 {",
            &[
                (Keyword, "for"),
                (Plain, " "),
                (Variable, "i"),
                (Plain, " "),
                (Keyword, "in"),
                (Plain, " "),
                (Number, "1"),
                (Blank, "___"),
                (Number, "5"),
                (Plain, " "),
                (Bracket, "{"),
            ],
        );
        check1(
            r#"let user = User___new("철수", 20);"#,
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "user"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Type, "User"),
                (Blank, "___"),
                (Function, "new"),
                (CallParen, "("),
                (Str, "\"철수\""),
                (Plain, ", "),
                (Number, "20"),
                (CallParen, ")"),
                (Plain, ";"),
            ],
        );
        check1(
            "let add = ___x: i32___ x + bonus;",
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "add"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Blank, "___"),
                (Variable, "x"),
                (Operator, ":"),
                (Plain, " "),
                (Type, "i32"),
                (Blank, "___"),
                (Plain, " "),
                (Variable, "x"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Variable, "bonus"),
                (Plain, ";"),
            ],
        );
        check1(
            "fn shout(s: ___String)",
            &[
                (Keyword, "fn"),
                (Plain, " "),
                (Function, "shout"),
                (Bracket, "("),
                (Variable, "s"),
                (Operator, ":"),
                (Plain, " "),
                (Blank, "___"),
                (Type, "String"),
                (Bracket, ")"),
            ],
        );
        check1(
            "let mid = ___arr[1..3];",
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "mid"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Blank, "___"),
                (Variable, "arr"),
                (Bracket, "["),
                (Number, "1"),
                (Operator, ".."),
                (Number, "3"),
                (Bracket, "]"),
                (Plain, ";"),
            ],
        );
        check1(
            r#"let pass = ___ score >= 60 { "합격" } ___ { "불합격" };"#,
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "pass"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Blank, "___"),
                (Plain, " "),
                (Variable, "score"),
                (Plain, " "),
                (Operator, ">="),
                (Plain, " "),
                (Number, "60"),
                (Plain, " "),
                (Bracket, "{"),
                (Plain, " "),
                (Str, "\"합격\""),
                (Plain, " "),
                (Bracket, "}"),
                (Plain, " "),
                (Blank, "___"),
                (Plain, " "),
                (Bracket, "{"),
                (Plain, " "),
                (Str, "\"불합격\""),
                (Plain, " "),
                (Bracket, "}"),
                (Plain, ";"),
            ],
        );
        check1("______", &[(Blank, "___"), (Blank, "___")]);
        check1(
            "-> ___<u32> {",
            &[
                (Operator, "->"),
                (Plain, " "),
                (Blank, "___"),
                (Bracket, "<"),
                (Type, "u32"),
                (Bracket, ">"),
                (Plain, " "),
                (Bracket, "{"),
            ],
        );
    }

    #[test]
    fn blank_in_comment_and_attribute() {
        check1(
            "// ___ 설명",
            &[(Comment, "// "), (Blank, "___"), (Comment, " 설명")],
        );
        check1(
            "#[derive(___)]",
            &[
                (Bracket, "#["),
                (Attribute, "derive"),
                (Bracket, "("),
                (Blank, "___"),
                (Bracket, ")]"),
            ],
        );
    }

    #[test]
    fn keywords_types_macros() {
        check1(
            "pub fn main() -> Option<String> {",
            &[
                (Keyword, "pub"),
                (Plain, " "),
                (Keyword, "fn"),
                (Plain, " "),
                (Function, "main"),
                (Bracket, "()"),
                (Plain, " "),
                (Operator, "->"),
                (Plain, " "),
                (Type, "Option"),
                (Bracket, "<"),
                (Type, "String"),
                (Bracket, ">"),
                (Plain, " "),
                (Bracket, "{"),
            ],
        );
        check1(
            "let v: Vec<u8> = vec![]; Some(self.x as usize)",
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "v"),
                (Operator, ":"),
                (Plain, " "),
                (Type, "Vec"),
                (Bracket, "<"),
                (Type, "u8"),
                (Bracket, ">"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Macro, "vec!"),
                (Bracket, "[]"),
                (Plain, "; "),
                (Variant, "Some"),
                (CallParen, "("),
                (SelfValue, "self"),
                (Operator, "."),
                (Variable, "x"),
                (Plain, " "),
                (Keyword, "as"),
                (Plain, " "),
                (Type, "usize"),
                (CallParen, ")"),
            ],
        );
        check1(
            r#"if a != b { panic!("x") }"#,
            &[
                (Keyword, "if"),
                (Plain, " "),
                (Variable, "a"),
                (Plain, " "),
                (Operator, "!="),
                (Plain, " "),
                (Variable, "b"),
                (Plain, " "),
                (Bracket, "{"),
                (Plain, " "),
                (Macro, "panic!"),
                (MacroParen, "("),
                (Str, "\"x\""),
                (MacroParen, ")"),
                (Plain, " "),
                (Bracket, "}"),
            ],
        );
        check1(
            "x!=y",
            &[(Variable, "x"), (Operator, "!="), (Variable, "y")],
        );
        check1(
            "impl Self",
            &[(Keyword, "impl"), (Plain, " "), (Type, "Self")],
        );
    }

    #[test]
    fn functions() {
        check1(
            "fn add(a: i32) {",
            &[
                (Keyword, "fn"),
                (Plain, " "),
                (Function, "add"),
                (Bracket, "("),
                (Variable, "a"),
                (Operator, ":"),
                (Plain, " "),
                (Type, "i32"),
                (Bracket, ")"),
                (Plain, " "),
                (Bracket, "{"),
            ],
        );
        check1(
            "x.len() + f(1)",
            &[
                (Variable, "x"),
                (Operator, "."),
                (Function, "len"),
                (CallParen, "()"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Function, "f"),
                (CallParen, "("),
                (Number, "1"),
                (CallParen, ")"),
            ],
        );
        check1(
            "String::from(s)",
            &[
                (Type, "String"),
                (Operator, "::"),
                (Function, "from"),
                (CallParen, "("),
                (Variable, "s"),
                (CallParen, ")"),
            ],
        );
        check1(
            "Vec::<i32>::new()",
            &[
                (Type, "Vec"),
                (Operator, "::"),
                (Bracket, "<"),
                (Type, "i32"),
                (Bracket, ">"),
                (Operator, "::"),
                (Function, "new"),
                (CallParen, "()"),
            ],
        );
        check1(
            "s.parse::<Vec<u8>>()",
            &[
                (Variable, "s"),
                (Operator, "."),
                (Function, "parse"),
                (Operator, "::"),
                (Bracket, "<"),
                (Type, "Vec"),
                (Bracket, "<"),
                (Type, "u8"),
                (Bracket, ">>"),
                (CallParen, "()"),
            ],
        );
        check1(
            "Some(Point(1))",
            &[
                (Variant, "Some"),
                (CallParen, "("),
                (Type, "Point"),
                (CallParen, "("),
                (Number, "1"),
                (CallParen, "))"),
            ],
        );
        // 이름과 `(` 사이에 공백이 있으면 호출로 보지 않는다
        check1(
            "x.len + f (1)",
            &[
                (Variable, "x"),
                (Operator, "."),
                (Variable, "len"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Variable, "f"),
                (Plain, " "),
                (Bracket, "("),
                (Number, "1"),
                (Bracket, ")"),
            ],
        );
        check1(
            "fn ___(x)",
            &[
                (Keyword, "fn"),
                (Plain, " "),
                (Blank, "___"),
                (Bracket, "("),
                (Variable, "x"),
                (Bracket, ")"),
            ],
        );
    }

    #[test]
    fn constants() {
        check1(
            "const MAX: u32 = true;",
            &[
                (Keyword, "const"),
                (Plain, " "),
                (Constant, "MAX"),
                (Operator, ":"),
                (Plain, " "),
                (Type, "u32"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Constant, "true"),
                (Plain, ";"),
            ],
        );
        check1(
            "PI * T::MAX_SIZE || false",
            &[
                (Constant, "PI"),
                (Plain, " "),
                (Operator, "*"),
                (Plain, " "),
                (Type, "T"),
                (Operator, "::"),
                (Constant, "MAX_SIZE"),
                (Plain, " "),
                (Operator, "||"),
                (Plain, " "),
                (Constant, "false"),
            ],
        );
    }

    #[test]
    fn operators() {
        check1(
            "match x { 1..=5 => &mut y?, _ => *z }",
            &[
                (Keyword, "match"),
                (Plain, " "),
                (Variable, "x"),
                (Plain, " "),
                (Bracket, "{"),
                (Plain, " "),
                (Number, "1"),
                (Operator, "..="),
                (Number, "5"),
                (Plain, " "),
                (Operator, "=>"),
                (Plain, " "),
                (Operator, "&"),
                (Keyword, "mut"),
                (Plain, " "),
                (Variable, "y"),
                (SpecialOperator, "?"),
                (Plain, ", "),
                (Keyword, "_"),
                (Plain, " "),
                (Operator, "=>"),
                (Plain, " "),
                (Operator, "*"),
                (Variable, "z"),
                (Plain, " "),
                (Bracket, "}"),
            ],
        );
        check1(
            "a < b && c >> 2 == d",
            &[
                (Variable, "a"),
                (Plain, " "),
                (Operator, "<"),
                (Plain, " "),
                (Variable, "b"),
                (Plain, " "),
                (Operator, "&&"),
                (Plain, " "),
                (Variable, "c"),
                (Plain, " "),
                (Operator, ">>"),
                (Plain, " "),
                (Number, "2"),
                (Plain, " "),
                (Operator, "=="),
                (Plain, " "),
                (Variable, "d"),
            ],
        );
        check1(
            "Box<dyn Fn(i32) -> i32>",
            &[
                (Type, "Box"),
                (Bracket, "<"),
                (Keyword, "dyn"),
                (Plain, " "),
                (Type, "Fn"),
                (Bracket, "("),
                (Type, "i32"),
                (Bracket, ")"),
                (Plain, " "),
                (Operator, "->"),
                (Plain, " "),
                (Type, "i32"),
                (Bracket, ">"),
            ],
        );
        check1(
            "|x| x += 1",
            &[
                (Bracket, "|"),
                (Variable, "x"),
                (Bracket, "|"),
                (Plain, " "),
                (Variable, "x"),
                (Plain, " "),
                (SpecialOperator, "+="),
                (Plain, " "),
                (Number, "1"),
            ],
        );
    }

    #[test]
    fn strings() {
        check1(
            r###""a\"b" + r#"c "d" e"# + b"f" + br"g""###,
            &[
                (Str, "\"a"),
                (Escape, r#"\""#),
                (Str, "b\""),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Str, r##"r#"c "d" e"#"##),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Str, r#"b"f""#),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Str, r#"br"g""#),
            ],
        );
        check1(
            "break bar",
            &[(Keyword, "break"), (Plain, " "), (Variable, "bar")],
        );
        check(
            "let s = \"a\nb\"; // c",
            &[
                &[
                    (Keyword, "let"),
                    (Plain, " "),
                    (Variable, "s"),
                    (Plain, " "),
                    (Operator, "="),
                    (Plain, " "),
                    (Str, "\"a"),
                ],
                &[(Str, "b\""), (Plain, "; "), (Comment, "// c")],
            ],
        );
    }

    #[test]
    fn escapes_and_interpolation() {
        check1(
            r#""a\nb {} {x:?} {{lit}}""#,
            &[
                (Str, "\"a"),
                (Escape, r"\n"),
                (Str, "b "),
                (Interpolation, "{}"),
                (Str, " "),
                (Interpolation, "{"),
                (Variable, "x"),
                (Interpolation, ":?}"),
                (Str, " {{lit}}\""),
            ],
        );
        check1(
            r#""{0}{name:>8} {:#?} {{{x}}}""#,
            &[
                (Str, "\""),
                (Interpolation, "{0}{"),
                (Variable, "name"),
                (Interpolation, ":>8}"),
                (Str, " "),
                (Interpolation, "{:#?}"),
                (Str, " {{"),
                (Interpolation, "{"),
                (Variable, "x"),
                (Interpolation, "}"),
                (Str, "}}\""),
            ],
        );
        check1(
            r#""\u{AC00}\x41\0\\\t'" + "\'""#,
            &[
                (Str, "\""),
                (Escape, r"\u{AC00}\x41\0\\\t"),
                (Str, "'\""),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Str, "\""),
                (Escape, r"\'"),
                (Str, "\""),
            ],
        );
        check1(
            r#"r"raw \n {x}" + b"b\n{x}""#,
            &[
                (Str, r#"r"raw \n {x}""#),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Str, "b\"b"),
                (Escape, r"\n"),
                (Str, "{x}\""),
            ],
        );
        check(
            "\"a \\\n  {x}\"",
            &[
                &[(Str, "\"a "), (Escape, "\\")],
                &[
                    (Str, "  "),
                    (Interpolation, "{"),
                    (Variable, "x"),
                    (Interpolation, "}"),
                    (Str, "\""),
                ],
            ],
        );
        check1(
            r#""{ \"k\" }""#,
            &[
                (Str, "\"{ "),
                (Escape, r#"\""#),
                (Str, "k"),
                (Escape, r#"\""#),
                (Str, " }\""),
            ],
        );
    }

    #[test]
    fn char_vs_lifetime() {
        check1(
            "fn f<'a>(x: &'static str)",
            &[
                (Keyword, "fn"),
                (Plain, " "),
                (Function, "f"),
                (Bracket, "<"),
                (Lifetime, "'a"),
                (Bracket, ">("),
                (Variable, "x"),
                (Operator, ":"),
                (Plain, " "),
                (Operator, "&"),
                (Lifetime, "'static"),
                (Plain, " "),
                (Type, "str"),
                (Bracket, ")"),
            ],
        );
        check1(
            r"['a', '\n', '\'', '가', b'x', b'\0', '\u{41}']",
            &[
                (Bracket, "["),
                (Char, "'a'"),
                (Plain, ", "),
                (Char, r"'\n'"),
                (Plain, ", "),
                (Char, r"'\''"),
                (Plain, ", "),
                (Char, "'가'"),
                (Plain, ", "),
                (Char, "b'x'"),
                (Plain, ", "),
                (Char, r"b'\0'"),
                (Plain, ", "),
                (Char, r"'\u{41}'"),
                (Bracket, "]"),
            ],
        );
    }

    #[test]
    fn numbers() {
        check1(
            "1_000u32 + 3.14 + 0xff + 1..=3 + x.0",
            &[
                (Number, "1_000u32"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Number, "3.14"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Number, "0xff"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Number, "1"),
                (Operator, "..="),
                (Number, "3"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Variable, "x"),
                (Operator, "."),
                (Variable, "0"),
            ],
        );
        check1("1..5", &[(Number, "1"), (Operator, ".."), (Number, "5")]);
        check1(
            "1e-3 2.5E+10 1e10f64 0.5..1.5",
            &[
                (Number, "1e-3"),
                (Plain, " "),
                (Number, "2.5E+10"),
                (Plain, " "),
                (Number, "1e10f64"),
                (Plain, " "),
                (Number, "0.5"),
                (Operator, ".."),
                (Number, "1.5"),
            ],
        );
        check1(
            "1usize-1 + t.0.1 + 0x1e-1",
            &[
                (Number, "1usize"),
                (Operator, "-"),
                (Number, "1"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Variable, "t"),
                (Operator, "."),
                (Variable, "0"),
                (Operator, "."),
                (Variable, "1"),
                (Plain, " "),
                (Operator, "+"),
                (Plain, " "),
                (Number, "0x1e"),
                (Operator, "-"),
                (Number, "1"),
            ],
        );
    }

    #[test]
    fn comments() {
        check1(
            r#"let x = 1; // 주석 "문자열 아님""#,
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "x"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Number, "1"),
                (Plain, "; "),
                (Comment, r#"// 주석 "문자열 아님""#),
            ],
        );
        check(
            "a /* x /* y */ \"z\"\nstill */ b",
            &[
                &[
                    (Variable, "a"),
                    (Plain, " "),
                    (Comment, "/* x /* y */ \"z\""),
                ],
                &[(Comment, "still */"), (Plain, " "), (Variable, "b")],
            ],
        );
    }

    #[test]
    fn attributes() {
        // `#[`, `]`, 괄호는 Bracket, 첫 이름은 Attribute, 인자는 대문자면 Type 소문자면 Variable
        check1(
            "#![allow(dead_code)]",
            &[
                (Bracket, "#!["),
                (Attribute, "allow"),
                (Bracket, "("),
                (Variable, "dead_code"),
                (Bracket, ")]"),
            ],
        );
        check1(
            "#[cfg(test)] mod t",
            &[
                (Bracket, "#["),
                (Attribute, "cfg"),
                (Bracket, "("),
                (Variable, "test"),
                (Bracket, ")]"),
                (Plain, " "),
                (Keyword, "mod"),
                (Plain, " "),
                (Module, "t"),
            ],
        );
        check(
            "#[derive(\n    Debug, Clone,\n)]\nstruct S;",
            &[
                &[(Bracket, "#["), (Attribute, "derive"), (Bracket, "(")],
                &[
                    (Plain, "    "),
                    (Type, "Debug"),
                    (Plain, ", "),
                    (Type, "Clone"),
                    (Plain, ","),
                ],
                &[(Bracket, ")]")],
                &[(Keyword, "struct"), (Plain, " "), (Type, "S"), (Plain, ";")],
            ],
        );
        check1(
            r#"#[doc = "a]\"[b"] fn f"#,
            &[
                (Bracket, "#["),
                (Attribute, "doc"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Str, "\"a]"),
                (Escape, r#"\""#),
                (Str, "[b\""),
                (Bracket, "]"),
                (Plain, " "),
                (Keyword, "fn"),
                (Plain, " "),
                (Function, "f"),
            ],
        );
        check1(
            "#[a[b]] x",
            &[
                (Bracket, "#["),
                (Attribute, "a"),
                (Bracket, "["),
                (Variable, "b"),
                (Bracket, "]]"),
                (Plain, " "),
                (Variable, "x"),
            ],
        );
    }

    #[test]
    fn raw_identifiers() {
        check1(
            "let r#type = r#match(1);",
            &[
                (Keyword, "let"),
                (Plain, " "),
                (Variable, "r#type"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Function, "r#match"),
                (CallParen, "("),
                (Number, "1"),
                (CallParen, ")"),
                (Plain, ";"),
            ],
        );
        check1(
            "fn r#fn()",
            &[
                (Keyword, "fn"),
                (Plain, " "),
                (Function, "r#fn"),
                (Bracket, "()"),
            ],
        );
    }

    #[test]
    fn variables_and_fields() {
        // 지역 변수, 매개변수, 클로저 매개변수, 필드 접근·정의·초기화, 패턴 바인딩
        kinds(
            "fn f(a: i32, mut b: &str) { let c = |d| d + a; p.x = c(b.len); }",
            &[
                ("a", Variable),
                ("mut", Keyword),
                ("b", Variable),
                ("c", Variable),
                ("d", Variable),
                ("d", Variable),
                ("a", Variable),
                ("p", Variable),
                ("x", Variable),
                ("c", Variable),
                ("(", CallParen),
                ("b", Variable),
                ("len", Variable),
            ],
        );
        kinds(
            "struct P { x: i32 } let p = P { x: 1, y }; let P { x, .. } = p;",
            &[
                ("x", Variable),
                ("p", Variable),
                ("x", Variable),
                ("y", Variable),
                ("x", Variable),
            ],
        );
        // 단독 `_` 는 와일드카드(Keyword), `_x` 는 변수
        kinds(
            "let _ = f(_x); Vec<_>",
            &[("_", Keyword), ("_x", Variable), ("_", Keyword)],
        );
    }

    #[test]
    fn modules() {
        kinds(
            "std::io::stdin(); let m = std::collections::HashMap::new(); fmt::Result",
            &[
                ("std", Module),
                ("io", Module),
                ("stdin", Function),
                ("std", Module),
                ("collections", Module),
                ("HashMap", Type),
                ("new", Function),
                ("fmt", Module),
                ("Result", Type),
            ],
        );
        // use 문의 소문자 마지막 경로는 변수(nvim @variable), 목록 안의 self 는 모듈
        check1(
            "use std::fmt;",
            &[
                (Keyword, "use"),
                (Plain, " "),
                (Module, "std"),
                (Operator, "::"),
                (Variable, "fmt"),
                (Plain, ";"),
            ],
        );
        kinds(
            "use std::io::{self, Read};\nlet x = y;",
            &[
                ("std", Module),
                ("io", Module),
                ("self", Module),
                ("Read", Type),
                ("x", Variable),
                ("y", Variable),
            ],
        );
        kinds(
            "mod tests { use super::*; } pub(crate) fn f() {}",
            &[("tests", Module), ("super", Module), ("crate", Keyword)],
        );
    }

    #[test]
    fn self_value() {
        kinds(
            "fn f(&self, other: &Self) -> Self { Self::new(self.x) }",
            &[
                ("self", SelfValue),
                ("Self", Type),
                ("Self", Type),
                ("Self", Type),
                ("new", Function),
                ("self", SelfValue),
                ("x", Variable),
            ],
        );
        kinds("self::helper()", &[("self", Module), ("helper", Function)]);
    }

    #[test]
    fn macro_parens() {
        // 매크로 이름과 `!` 는 Macro, `name!(` 의 최상위 괄호만 MacroParen
        check1(
            r#"println!("{}", f(x));"#,
            &[
                (Macro, "println!"),
                (MacroParen, "("),
                (Str, "\""),
                (Interpolation, "{}"),
                (Str, "\""),
                (Plain, ", "),
                (Function, "f"),
                (CallParen, "("),
                (Variable, "x"),
                (CallParen, ")"),
                (MacroParen, ")"),
                (Plain, ";"),
            ],
        );
        check1(
            "vec![(1, 2)]; m!{}",
            &[
                (Macro, "vec!"),
                (Bracket, "[("),
                (Number, "1"),
                (Plain, ", "),
                (Number, "2"),
                (Bracket, ")]"),
                (Plain, "; "),
                (Macro, "m!"),
                (Bracket, "{}"),
            ],
        );
    }

    #[test]
    fn call_parens() {
        kinds(
            "foo(1); x.bar(); Vec::new(); s.parse::<i32>(); Some(5); Ok(x); Point(1, 2); Self(0);",
            &[
                ("foo", Function),
                ("(", CallParen),
                (")", CallParen),
                ("bar", Function),
                ("()", CallParen),
                ("new", Function),
                ("()", CallParen),
                ("parse", Function),
                ("()", CallParen),
                ("Some", Variant),
                ("(", CallParen),
                (")", CallParen),
                ("Ok", Variant),
                ("(", CallParen),
                (")", CallParen),
                ("Point", Type),
                ("(", CallParen),
                (")", CallParen),
                ("Self", Type),
                ("(", CallParen),
            ],
        );
        // fn 정의의 매개변수 괄호, 튜플, 그룹 괄호는 Bracket
        kinds(
            "fn f(x: (i32, i32)) { let t = (1 + 2) * 3; }",
            &[
                ("(", Bracket),
                ("(", Bracket),
                ("))", Bracket),
                ("{", Bracket),
                ("(", Bracket),
                (")", Bracket),
                ("}", Bracket),
            ],
        );
    }

    #[test]
    fn pattern_parens_are_brackets() {
        // let/if let/while let/for 패턴의 생성자 괄호는 Bracket, `=` 뒤의 식은 CallParen
        check1(
            "if let Some(x) = Some(1) {",
            &[
                (Keyword, "if"),
                (Plain, " "),
                (Keyword, "let"),
                (Plain, " "),
                (Variant, "Some"),
                (Bracket, "("),
                (Variable, "x"),
                (Bracket, ")"),
                (Plain, " "),
                (Operator, "="),
                (Plain, " "),
                (Variant, "Some"),
                (CallParen, "("),
                (Number, "1"),
                (CallParen, ")"),
                (Plain, " "),
                (Bracket, "{"),
            ],
        );
        kinds(
            "while let Some(top) = stack.pop() {}",
            &[("(", Bracket), (")", Bracket), ("()", CallParen)],
        );
        kinds(
            "let Ok(n): Result<i32, E> = Ok(1) else { return };",
            &[
                ("(", Bracket),
                (")", Bracket),
                ("Ok", Variant),
                ("(", CallParen),
                (")", CallParen),
            ],
        );
        kinds(
            "for (i, Wrap(x)) in v.iter().enumerate() {}",
            &[
                ("(", Bracket),
                ("(", Bracket),
                ("))", Bracket),
                ("iter", Function),
                ("()", CallParen),
            ],
        );
        // impl 헤더의 for, HRTB 의 for<'a> 는 패턴을 열지 않는다
        kinds(
            "impl From<i32> for W { fn from(n: i32) -> Self { W(n) } }",
            &[
                ("from", Function),
                ("(", Bracket),
                ("Self", Type),
                ("W", Type),
                ("(", CallParen),
            ],
        );
        // 매크로 안의 생성자는 식 자리
        kinds(
            "assert_eq!(f(), Some(1));",
            &[("Some", Variant), ("(", CallParen)],
        );
    }

    #[test]
    fn match_arms() {
        check(
            "match opt {\n    Some(n) if n > 0 => Some(n),\n    Some(Point(x, _)) => {\n        bar(x)\n    }\n    None => None,\n}",
            &[
                &[
                    (Keyword, "match"),
                    (Plain, " "),
                    (Variable, "opt"),
                    (Plain, " "),
                    (Bracket, "{"),
                ],
                &[
                    (Plain, "    "),
                    (Variant, "Some"),
                    (Bracket, "("),
                    (Variable, "n"),
                    (Bracket, ")"),
                    (Plain, " "),
                    (Keyword, "if"),
                    (Plain, " "),
                    (Variable, "n"),
                    (Plain, " "),
                    (Operator, ">"),
                    (Plain, " "),
                    (Number, "0"),
                    (Plain, " "),
                    (Operator, "=>"),
                    (Plain, " "),
                    (Variant, "Some"),
                    (CallParen, "("),
                    (Variable, "n"),
                    (CallParen, ")"),
                    (Plain, ","),
                ],
                &[
                    (Plain, "    "),
                    (Variant, "Some"),
                    (Bracket, "("),
                    (Type, "Point"),
                    (Bracket, "("),
                    (Variable, "x"),
                    (Plain, ", "),
                    (Keyword, "_"),
                    (Bracket, "))"),
                    (Plain, " "),
                    (Operator, "=>"),
                    (Plain, " "),
                    (Bracket, "{"),
                ],
                &[
                    (Plain, "        "),
                    (Function, "bar"),
                    (CallParen, "("),
                    (Variable, "x"),
                    (CallParen, ")"),
                ],
                &[(Plain, "    "), (Bracket, "}")],
                &[
                    (Plain, "    "),
                    (Variant, "None"),
                    (Plain, " "),
                    (Operator, "=>"),
                    (Plain, " "),
                    (Variant, "None"),
                    (Plain, ","),
                ],
                &[(Bracket, "}")],
            ],
        );
        // 블록 팔 뒤 쉼표 없이 오는 팔, 경로 패턴, struct 패턴
        kinds(
            "match m { E::A(x) => { f(x) } E::B { v } => E::A(v), _ => g(Ok(1)) }",
            &[
                ("A", Variant),
                ("(", Bracket),
                ("f", Function),
                ("(", CallParen),
                ("B", Variant),
                ("v", Variable),
                ("A", Variant),
                ("(", CallParen),
                ("_", Keyword),
                ("g", Function),
                ("(", CallParen),
                ("Ok", Variant),
                ("(", CallParen),
            ],
        );
    }

    #[test]
    fn definition_parens_are_brackets() {
        kinds(
            "enum E { A(i32), B { x: i32 } } struct P(i32, String); let p = P(1, s);",
            &[
                ("A", Variant),
                ("(", Bracket),
                ("x", Variable),
                ("P", Type),
                ("(", Bracket),
                ("P", Type),
                ("(", CallParen),
            ],
        );
        kinds(
            "fn apply(f: impl Fn(i32) -> i32, g: &dyn FnMut(u8)) {}",
            &[
                ("Fn", Type),
                ("(", Bracket),
                ("FnMut", Type),
                ("(", Bracket),
            ],
        );
    }

    #[test]
    fn brackets_pair_across_lines() {
        check(
            "let x = foo(\n    1,\n    [2, 3],\n);",
            &[
                &[
                    (Keyword, "let"),
                    (Plain, " "),
                    (Variable, "x"),
                    (Plain, " "),
                    (Operator, "="),
                    (Plain, " "),
                    (Function, "foo"),
                    (CallParen, "("),
                ],
                &[(Plain, "    "), (Number, "1"), (Plain, ",")],
                &[
                    (Plain, "    "),
                    (Bracket, "["),
                    (Number, "2"),
                    (Plain, ", "),
                    (Number, "3"),
                    (Bracket, "]"),
                    (Plain, ","),
                ],
                &[(CallParen, ")"), (Plain, ";")],
            ],
        );
        check(
            "println!(\n    \"{}\",\n    x,\n);",
            &[
                &[(Macro, "println!"), (MacroParen, "(")],
                &[
                    (Plain, "    "),
                    (Str, "\""),
                    (Interpolation, "{}"),
                    (Str, "\""),
                    (Plain, ","),
                ],
                &[(Plain, "    "), (Variable, "x"), (Plain, ",")],
                &[(MacroParen, ")"), (Plain, ";")],
            ],
        );
    }

    #[test]
    fn closure_pipes_and_generics_are_brackets() {
        // 값 뒤가 아닌 `|` 는 클로저 매개변수 괄호, 값 뒤의 `|`, `||` 는 연산자
        kinds(
            "let f = |a, (b, c)| a; v.map(move |x| x); g(|| 1); a || b | c; let h = |&x| x;",
            &[
                ("|", Bracket),
                ("a", Variable),
                (")|", Bracket),
                ("move", Keyword),
                ("|", Bracket),
                ("|", Bracket),
                ("||", Bracket),
                ("||", Operator),
                ("|", Operator),
                ("|", Bracket),
                ("|", Bracket),
            ],
        );
        kinds(
            "match x { A | B => 1, Some(1) | None => 2, _ | _ => 3 } x |= 1;",
            &[
                ("|", Operator),
                ("|", Operator),
                ("|", Operator),
                ("|=", SpecialOperator),
            ],
        );
        // 붙여 쓴 제네릭 `<..>` 는 Bracket, 띄어 쓴 비교는 Operator
        kinds(
            "fn f<T>(x: Vec<T>) -> Option<Vec<u8>> { if a < b && c > d {} }",
            &[
                ("<", Bracket),
                (">(", Bracket),
                ("<", Bracket),
                (">)", Bracket),
                ("<", Bracket),
                ("<", Bracket),
                (">>", Bracket),
                ("<", Operator),
                (">", Operator),
            ],
        );
    }

    #[test]
    fn closure_binding_calls_are_variables() {
        // `let name = [move] |..|` 로 묶은 클로저 호출은 변수, 괄호는 호출 괄호
        kinds(
            "let add_one = |x| x + 1;\nlet mut g = move || 2;\nadd_one(5) + g() + h(1) + v.add_one(2)",
            &[
                ("add_one", Variable),
                ("g", Variable),
                ("add_one", Variable),
                ("(", CallParen),
                ("g", Variable),
                ("()", CallParen),
                ("h", Function),
                ("add_one", Function),
            ],
        );
    }

    #[test]
    fn enum_variants() {
        // Some/None/Ok/Err, enum 본문의 변형, 대문자 타입 경로 뒤 `Enum::Variant` 는 Variant
        kinds(
            "enum Shape { Circle(f64), Rect { w: u32 }, Empty } let s = Shape::Rect { w: 1 };",
            &[
                ("Shape", Type),
                ("Circle", Variant),
                ("f64", Type),
                ("Rect", Variant),
                ("u32", Type),
                ("Empty", Variant),
                ("Shape", Type),
                ("Rect", Variant),
                ("w", Variable),
            ],
        );
        kinds(
            "match s { Shape::Circle(r) => Option::Some(r), Shape::Rect { w } | Shape::Empty => Err(e) }",
            &[
                ("Circle", Variant),
                ("(", Bracket),
                ("Option", Type),
                ("Some", Variant),
                ("(", CallParen),
                ("Rect", Variant),
                ("w", Variable),
                ("Empty", Variant),
                ("Err", Variant),
                ("(", CallParen),
            ],
        );
        // 경로 중간의 대문자, 소문자 경로 뒤, ALL_CAPS 는 변형이 아니다
        kinds(
            "a::B::c(); io::Error::new(); T::MAX; Some",
            &[
                ("B", Type),
                ("Error", Type),
                ("T", Type),
                ("MAX", Constant),
                ("Some", Variant),
            ],
        );
    }

    #[test]
    fn special_operators() {
        // `?`(값 뒤), 복합 대입은 SpecialOperator. `?Sized` 의 `?` 와 매크로 인자 안 연산자는 Operator
        kinds(
            "x += 1; y <<= 2; let n = s.parse()?; fn f<T: ?Sized>() {} println!(\"{}\", a.b + &c::d(-1)); let z = a.b + 1;",
            &[
                ("+=", SpecialOperator),
                ("<<=", SpecialOperator),
                ("?", SpecialOperator),
                ("?", Operator),
                (".", Operator),
                ("+", Operator),
                ("&", Operator),
                ("::", Operator),
                ("-", Operator),
                ("1", Number),
                (".", Operator),
                ("+", Operator),
            ],
        );
    }

    #[test]
    fn placeholder_names_are_variables() {
        // `{name}` 의 name 은 대소문자와 무관하게 변수, `{0}` `{:?}` 는 자리표시자
        check1(
            r#""{GREETING} {n:>4} {0} {:?} {___}""#,
            &[
                (Str, "\""),
                (Interpolation, "{"),
                (Variable, "GREETING"),
                (Interpolation, "}"),
                (Str, " "),
                (Interpolation, "{"),
                (Variable, "n"),
                (Interpolation, ":>4}"),
                (Str, " "),
                (Interpolation, "{0}"),
                (Str, " "),
                (Interpolation, "{:?}"),
                (Str, " "),
                (Interpolation, "{"),
                (Blank, "___"),
                (Interpolation, "}"),
                (Str, "\""),
            ],
        );
    }
}
