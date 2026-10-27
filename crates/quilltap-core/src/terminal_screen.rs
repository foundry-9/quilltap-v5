//! Terminal screen model — a port of v4's `lib/terminal/screen.ts` (bug 255,
//! v4 `a6292f169`; unmoved to the `322a5488e` pin), over the `vt100` crate
//! where v4 runs `@xterm/headless` 6.0.0.
//!
//! A server-side terminal emulator keeps the same picture of a PTY session the
//! operator's xterm.js pane shows. Text read back for a non-terminal consumer —
//! the `terminal_read` tool, Ariel's periodic summaries — is rendered from this
//! model rather than by stripping escape codes from the raw byte stream.
//!
//! v4's why, carried: stripping works for programs that write line by line,
//! but a full-screen program (Claude Code, vim, htop, less) paints its screen
//! with cursor addressing, cursor-right moves in place of spaces, and in-place
//! redraws on the alternate screen. Strip those codes and nothing says where
//! any text belongs: 422 KB of Claude Code output cleaned down to 526
//! characters of fragments (bug 255). An emulator applies them instead.
//!
//! The emulator never answers the program: v4 does not wire xterm's `onData`
//! (replies to device-status queries and the like) to the PTY — the
//! operator's browser terminal already answers those. `vt100` has no reply
//! channel at all, so there is nothing to leave unwired here.
//!
//! ## The text model is xterm's, reproduced over vt100's cells
//!
//! What a reader gets is decided by v4's `readLines` over xterm's
//! `IBufferLine.translateToString`, so that is what this module reproduces —
//! never `vt100`'s own `contents()` / `rows()`, which omit never-written cells
//! and would lose `translateToString(false)`'s padding:
//!
//! - `translateToString(false)` emits a SPACE for every cell without content
//!   (never written, or erased) and skips a wide character's continuation
//!   cell; `translateToString(true)` stops after the last cell WITH content
//!   (a written space has content and survives), then v4 applies JS
//!   `trimEnd()` — "xterm's trimRight drops only never-written cells;
//!   full-screen programs pad with written spaces, so trim those too."
//! - xterm flags a row `isWrapped` when it CONTINUES the row above; vt100
//!   flags a row `wrapped` when it continues INTO the row below. So xterm's
//!   `isWrapped(y)` is vt100's `wrapped(y - 1)`, and for the buffer's first
//!   held line it is the wrap flag of the row that last left the scrollback
//!   (tracked by the `on_scroll` callback).
//! - v4's `joinWrapped` rationale: soft-wrapped rows are joined back onto the
//!   row they continue — right for a shell's scrollback, wrong for a
//!   full-screen program's screen, which is a grid: a program that fills a row
//!   to the edge and then repositions the cursor leaves the wrap flag on the
//!   next row, and joining would run the whole screen into one line.
//! - xterm's buffer is a circular list with no bounds check: once the normal
//!   buffer holds its full `rows + scrollback` lines, `getLine(length)` wraps
//!   round to line 0, so the LAST line's `continues` reads line 0's wrap flag
//!   (measured — the corpus's `edge-wrapped-first-row`). Reproduced. NOT
//!   reproduced (recorded, no corpus row): after `ESC[3J` on a buffer that once
//!   filled, `getLine(length)` reads a STALE slot of the circular list.
//!
//! **Scrollback wrap flags are read without mutating the emulator.** `vt100`
//! reads scrollback only through `set_scrollback`, which grows every viewed row
//! to the screen width — and `Row::grow` CLEARS the row's wrap flag. A row
//! pushed into scrollback is trimmed of trailing blank cells first, so a
//! wrapped row ending in a never-written cell (a wide character that did not
//! fit at the right edge) would lose its join the moment it was read. So the
//! cells are read from a CLONE of the screen, and the scrollback rows' wrap
//! flags from `write_contents_formatted_basic(Full)` over the untouched
//! original, which separates exactly the non-wrapped rows with `\n`.
//!
//! ## The summary marker
//!
//! v4 `registerMarker(0)`s the cursor's absolute line (scrollback held + the
//! cursor row) at every plain-shell summary; xterm moves the marker with its
//! line as lines scroll out of the scrollback and DISPOSES it when its line
//! leaves, after which the next summary reads from 0. Here the marker is an
//! absolute index over every line the normal buffer has ever held: the number
//! of lines dropped off the scrollback's top (`on_scroll`) plus the line's
//! index in the buffer. It is registered BEFORE the null decision, so it
//! advances even over an all-blank window.
//!
//! ## No twin
//!
//! - `settle()` — xterm parses writes asynchronously and every v4 read awaits
//!   `settle()` first; `vt100` parses synchronously, so `render` and
//!   `take_summary` are plain `fn`s and there is no observable difference.
//! - `dispose()` — `Drop`.
//! - The WARN `[TerminalScreen] Failed to resize emulator` `{ cols, rows,
//!   error }` (`module: 'terminal-screen'`) is UNREACHABLE through v4's real
//!   code for a number: xterm 6.0.0's `resize` returns on `NaN` and otherwise
//!   clamps to `cols >= 2`, `rows >= 1` (`MINIMUM_COLS` / `MINIMUM_ROWS`,
//!   measured in the pin's bundle) and does not throw. v5's `resize` takes
//!   `u16`s, applies the same clamp, and cannot fail — the line has no v5
//!   home (pinned NEGATIVE by the module test `resize_never_fails`).
//!
//! ## What vt100 does differently on the way in — normalised in `write`
//!
//! Each of these was MEASURED by `terminal_screen_equivalence`'s corpus against
//! v4's real `screen.ts`, and is reproduced by rewriting the input before vt100
//! parses it (vt100 exposes no hook inside its parser):
//!
//! - **Character widths.** xterm sizes characters by its Unicode 6 table
//!   (`UnicodeV6`, the default version) — every emoji measured (⏺ 😀 ⭐ ☕ ⚡
//!   🔥) is ONE cell there; vt100 sizes them by `unicode-width`, where most are
//!   two, and it drops U+FFFD outright. A character whose widths differ is fed
//!   to vt100 as a private stand-in of xterm's width and mapped back when read
//!   (`Proxies`); U+FE0F is always stood in for, since vt100 widens the cell an
//!   emoji-presentation selector follows and xterm never does.
//! - **C1 controls and `ESC D` / `ESC E`.** xterm executes them (index, next
//!   line, reverse index, the introducers); vt100 drops C1 and ignores those
//!   two escapes (`normalize_controls`).
//! - **A wide character at the right edge.** xterm wraps it and flags the
//!   continuation; vt100 flags the wrap only when the last cell has content.
//! - **`?47` / `?1047` / `?1049` h / l.** xterm carries the cursor across the
//!   switch and treats a repeat entry as a no-op; vt100 homes the cursor and
//!   re-clears the alternate screen on every `?1049h`.
//! - **`ED 0 / 1 / 2` disposes the summary marker** on any line it resets
//!   (xterm's `_resetBufferLine` → `clearMarkers`).
//!
//! An escape sequence split across two writes is held back until it completes
//! (an incomplete sequence draws nothing, so holding it changes no reading).
//!
//! ## Recorded divergences (measured, not planned — named both-ways tables)
//!
//! - `SCREEN_ERASE_UNLINKS_CONTINUATION` — xterm keeps its wrap flag on the
//!   line that CONTINUES the one above and clears it when an erase starts at
//!   column 0 (`EL 0` at the left margin, `EL 2`); vt100 keeps it on the row
//!   that wraps INTO the next and offers no way to clear it from outside. A
//!   soft-wrapped line rewritten from its start (`\r ESC[K new text`) stays
//!   joined to the line above here. Editing inside a wrapped line (`ICH` /
//!   `DCH`) and a readline-style long command both match.
//! - `SCREEN_MARKER_LINE_SPLICE` — xterm moves (or disposes) the summary
//!   marker when lines are inserted or deleted inside the screen (`IL` / `DL`
//!   and scroll-region scrolls splice its circular buffer); here only the
//!   scrollback's drops and the `ED` resets move it, so a plain-shell summary
//!   after an `IL` above the marker starts one line early.
//! - Not reproduced and not in the corpus (recorded): `HTS` tab stops (vt100
//!   keeps fixed 8-column stops); a C1 control inside an OSC / DCS string
//!   (rewritten as if printed); vt100 caps a cell at ~18 bytes of combining
//!   marks where xterm keeps them all; xterm's stale circular-buffer slot read
//!   after `ESC[3J` on a buffer that once filled.
//!
//! ## Recorded divergence — `SCREEN_RESIZE_NO_REFLOW` (R-D)
//!
//! xterm REFLOWS the normal buffer on a column change (a long line re-wraps at
//! the new width); `vt100` truncates or pads each row and clears its wrap
//! flag. A live session resized narrower therefore renders truncated rows
//! where v4 re-wraps, and one resized wider renders the old wrapped halves as
//! two lines where v4 joins them. `render_transcript` never resizes, so an
//! exited session's read is unaffected. Pinned both ways in
//! `terminal_screen_equivalence`'s `SCREEN_RESIZE_NO_REFLOW` table.

use std::collections::HashMap;
use std::num::NonZeroU16;

use serde::Serialize;

use crate::jsstr::{js_trim, js_trim_end};

/// Lines of normal-buffer scrollback the server-side emulator keeps.
pub const SCREEN_SCROLLBACK_LINES: usize = 5000;

/// xterm's `MINIMUM_COLS` / `MINIMUM_ROWS`, applied by its constructor and its
/// `resize` alike.
const MINIMUM_COLS: u16 = 2;
const MINIMUM_ROWS: u16 = 1;

/// Which xterm buffer a rendering came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ScreenBufferKind {
    Normal,
    Alternate,
}

impl ScreenBufferKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ScreenBufferKind::Normal => "normal",
            ScreenBufferKind::Alternate => "alternate",
        }
    }
}

/// The screen as it stands. `Alternate` means a full-screen program owns the
/// terminal and `lines` is its current screen (there is no scrollback on the
/// alternate screen); `Normal` means `lines` is the shell's scrollback plus the
/// visible rows. Serialises to v4's `{ buffer, lines, cols, rows }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScreenRendering {
    pub buffer: ScreenBufferKind,
    pub lines: Vec<String>,
    pub cols: u16,
    pub rows: u16,
}

/// What kind of summary Ariel posts: a plain shell's new output, or a
/// full-screen program's current screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SummaryKind {
    #[default]
    Output,
    Screen,
}

impl SummaryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SummaryKind::Output => "output",
            SummaryKind::Screen => "screen",
        }
    }
}

/// What Ariel should post for a summary (`None` from
/// [`TerminalScreen::take_summary`] when there is nothing new). Serialises to
/// v4's `{ kind, text }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ScreenSummary {
    pub kind: SummaryKind,
    pub text: String,
}

/// Counts the lines that leave the normal buffer's scrollback (the summary
/// marker's disposal rule) and remembers whether the last one wrapped into the
/// line now first held (xterm's `isWrapped` on line 0).
#[derive(Debug, Default)]
struct ScrollTracker {
    dropped: u64,
    top_is_continuation: bool,
}

impl vt100::Callbacks for ScrollTracker {
    fn on_scroll(&mut self, contents: vt100::capture::RowContents<'_>, alternate_screen: bool) {
        if alternate_screen {
            return;
        }
        self.dropped += 1;
        // `RowContents` exposes the row's wrap flag only through the basic
        // capture's pending newline: a second write of the same row starts with
        // `\n` exactly when the row did NOT wrap.
        let mut state = vt100::capture::BasicFormattedCaptureState::new();
        let mut scratch = String::new();
        let _ = contents.write_formatted_basic(&mut scratch, &mut state);
        scratch.clear();
        let _ = contents.write_formatted_basic(&mut scratch, &mut state);
        self.top_is_continuation = !scratch.starts_with('\n');
    }
}

/// One buffer line as xterm's `translateToString` renders it.
struct LineText {
    /// `translateToString(false)` — every cell, blanks as spaces.
    full: String,
    /// `translateToString(true)` — through the last cell with content.
    trimmed: String,
}

/// A buffer's lines plus xterm's wrap view of them.
struct BufferSnapshot {
    lines: Vec<LineText>,
    /// vt100's sense: `wraps_into_next[y]` = line `y` continues onto `y + 1`.
    wraps_into_next: Vec<bool>,
    /// xterm's `isWrapped` on line 0.
    first_is_continuation: bool,
    /// The normal buffer holds its full `rows + scrollback` lines, so xterm's
    /// circular `getLine(length)` answers line 0.
    full: bool,
}

impl BufferSnapshot {
    /// xterm's `buffer.getLine(y)?.isWrapped === true`, including the
    /// circular read one past the end.
    fn is_wrapped(&self, y: usize) -> bool {
        let len = self.lines.len();
        if y == 0 {
            self.first_is_continuation
        } else if y < len {
            self.wraps_into_next[y - 1]
        } else if y == len && self.full {
            self.first_is_continuation
        } else {
            false
        }
    }

    /// v4's `readLines(buffer, from, to, joinWrapped)`, verbatim in shape.
    fn read_lines(&self, from: usize, to: usize, join_wrapped: bool) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let to = to.min(self.lines.len());
        for y in from..to {
            let line = &self.lines[y];
            let continues = join_wrapped && self.is_wrapped(y + 1);
            let text = if continues {
                line.full.clone()
            } else {
                js_trim_end(&line.trimmed).to_string()
            };
            if join_wrapped && self.is_wrapped(y) && y > from && !out.is_empty() {
                if let Some(last) = out.last_mut() {
                    last.push_str(&text);
                }
            } else {
                out.push(text);
            }
        }
        if out.last().is_some_and(|l| js_trim(l).is_empty()) {
            // Trim the run of blank rows below the last written line.
            while out.last().is_some_and(|l| js_trim(l).is_empty()) {
                out.pop();
            }
        }
        out
    }
}

/// `Cell == Cell::DEFAULT`, from the public getters (vt100's `DEFAULT` is
/// crate-private): no contents, no wide flags, default colours and modes.
fn is_default_cell(cell: &vt100::Cell) -> bool {
    !cell.has_contents()
        && !cell.is_wide()
        && !cell.is_wide_continuation()
        && cell.fgcolor() == vt100::Color::Default
        && cell.bgcolor() == vt100::Color::Default
        && !cell.bold()
        && !cell.dim()
        && !cell.italic()
        && !cell.underline()
        && !cell.inverse()
}

/// Render visible row `row` of `screen` as xterm would, plus vt100's
/// basic-capture text of it (blank cells before content as spaces, trailing
/// default cells omitted, SGR omitted) — the key the wrap-flag parse matches.
fn render_row(
    screen: &vt100::Screen,
    row: u16,
    cols: u16,
    proxies: &Proxies,
) -> (LineText, String) {
    let mut full = String::new();
    let mut content_end = 0usize;
    let mut basic = String::new();
    let mut pending_blank = 0usize;
    let mut skip_next = false;
    for col in 0..cols {
        let cell = screen.cell(row, col);
        if std::mem::take(&mut skip_next) {
            continue;
        }
        let Some(cell) = cell else {
            full.push(' ');
            pending_blank += 1;
            continue;
        };
        skip_next = cell.is_wide();
        if cell.has_contents() {
            proxies.unmap_into(cell.contents(), &mut full);
            content_end = full.len();
        } else {
            full.push(' ');
        }
        if is_default_cell(cell) {
            pending_blank += 1;
        } else {
            basic.extend(std::iter::repeat_n(' ', std::mem::take(&mut pending_blank)));
            if cell.has_contents() {
                // The basic capture writes the cell's raw (proxied) contents.
                basic.push_str(cell.contents());
            } else {
                basic.push(' ');
            }
        }
    }
    let trimmed = full[..content_end].to_string();
    (LineText { full, trimmed }, basic)
}

/// Remove vt100's attribute escapes (`ESC [ <digits / ;> m` — the only escape
/// `write_contents_formatted_basic` emits).
fn strip_sgr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for d in chars.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The wrap flags of the scrollback rows, from the untouched screen's basic
/// capture: between consecutive rows a `\n` is written exactly when the earlier
/// row did NOT wrap. `basics[y]` is each scrollback row's basic text. Answers
/// `None` if the stream does not parse (never expected).
fn scrollback_wrap_flags(original: &vt100::Screen, basics: &[String]) -> Option<Vec<bool>> {
    let mut raw = String::new();
    let mut state = vt100::capture::BasicFormattedCaptureState::new();
    original
        .write_contents_formatted_basic(
            &mut raw,
            vt100::capture::BasicFormattedCaptureRange::Full(&mut state),
        )
        .ok()?;
    let stream = strip_sgr(&raw);
    let mut rest = stream.as_str();
    let mut flags = Vec::with_capacity(basics.len());
    for (y, basic) in basics.iter().enumerate() {
        if y > 0 {
            match rest.strip_prefix('\n') {
                Some(r) => {
                    flags.push(false);
                    rest = r;
                }
                None => flags.push(true),
            }
        }
        rest = rest.strip_prefix(basic.as_str())?;
    }
    // The last scrollback row: is there a separator before the first screen row?
    if !basics.is_empty() {
        flags.push(!rest.starts_with('\n'));
    }
    Some(flags)
}

// ---------------------------------------------------------------------------
// The input normaliser — what vt100 does differently from xterm on the way in
// ---------------------------------------------------------------------------

/// The longest escape tail held back for the next write.
const CARRY_LIMIT: usize = 256;

/// C1 controls (U+0080–U+009F) and the 7-bit `ESC D` / `ESC E`, rewritten to
/// what vt100 executes. xterm runs C1 controls (`IND`, `NEL`, `RI` and the
/// string / sequence introducers) and `ESC D` (index) / `ESC E` (next line);
/// vt100 drops every C1 character (`unhandled_char`) and ignores `ESC D` /
/// `ESC E`. Index is a bare line feed here (xterm's `convertEol` is off), next
/// line a carriage return then a line feed; the introducers become their 7-bit
/// `ESC` forms; the C1 controls xterm does not act on are dropped, as there.
/// `HTS` (`U+0088` / `ESC H`) sets no tab stop in vt100 — not reproduced.
fn normalize_controls(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.peek() {
                Some('D') => {
                    chars.next();
                    out.push('\n');
                }
                Some('E') => {
                    chars.next();
                    out.push_str("\r\n");
                }
                _ => out.push(c),
            },
            '\u{84}' => out.push('\n'),
            '\u{85}' => out.push_str("\r\n"),
            '\u{8d}' => out.push_str("\u{1b}M"),
            '\u{90}' => out.push_str("\u{1b}P"),
            '\u{98}' => out.push_str("\u{1b}X"),
            '\u{9b}' => out.push_str("\u{1b}["),
            '\u{9c}' => out.push_str("\u{1b}\\"),
            '\u{9d}' => out.push_str("\u{1b}]"),
            '\u{9e}' => out.push_str("\u{1b}^"),
            '\u{9f}' => out.push_str("\u{1b}_"),
            '\u{80}'..='\u{9f}' => {}
            _ => out.push(c),
        }
    }
    out
}

/// What [`scan_escape`] found at an `ESC`.
enum Scan {
    /// The input ends inside a possible control sequence.
    Incomplete,
    /// A complete `CSI` (`ESC [ <params> <intermediates> <final>`) of `len`
    /// chars.
    Csi {
        len: usize,
        private: Option<char>,
        params: String,
        intermediates: String,
        final_byte: char,
    },
    /// Anything else — left to the parser.
    Other,
}

fn scan_escape(chars: &[char]) -> Scan {
    match chars.get(1) {
        None => return Scan::Incomplete,
        Some('[') => {}
        Some(_) => return Scan::Other,
    }
    let mut i = 2;
    let private = match chars.get(i) {
        Some(c @ ('<' | '=' | '>' | '?')) => {
            i += 1;
            Some(*c)
        }
        _ => None,
    };
    let mut params = String::new();
    while let Some(&c) = chars.get(i) {
        if ('0'..='?').contains(&c) {
            params.push(c);
            i += 1;
        } else {
            break;
        }
    }
    let mut intermediates = String::new();
    while let Some(&c) = chars.get(i) {
        if (' '..='/').contains(&c) {
            intermediates.push(c);
            i += 1;
        } else {
            break;
        }
    }
    match chars.get(i) {
        None => Scan::Incomplete,
        Some(&c) if ('@'..='~').contains(&c) => Scan::Csi {
            len: i + 1,
            private,
            params,
            intermediates,
            final_byte: c,
        },
        Some(_) => Scan::Other,
    }
}

/// A control sequence this screen runs itself.
enum Intercept {
    EraseInDisplay(u16),
    AltScreen { mode: u16, set: bool },
}

fn classify_csi(private: Option<char>, params: &str, final_byte: char) -> Option<Intercept> {
    match (private, final_byte) {
        (None | Some('?'), 'J') => {
            // xterm reads `params[0]` (absent → 0).
            let first = params.split(';').next().unwrap_or("");
            let mode = if first.is_empty() {
                0
            } else {
                first.parse().ok()?
            };
            Some(Intercept::EraseInDisplay(mode))
        }
        (Some('?'), 'h' | 'l') => match params {
            "47" | "1047" | "1049" => Some(Intercept::AltScreen {
                mode: params.parse().ok()?,
                set: final_byte == 'h',
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Width-correct stand-ins. xterm sizes characters by its Unicode 6 table
/// ([`xterm_wcwidth`] — every emoji is ONE cell there); vt100 sizes them by
/// `unicode-width` (most emoji are two) and drops U+FFFD outright. A character
/// whose two widths differ is fed to vt100 as a private stand-in of xterm's
/// width and mapped back when the screen is read:
///
/// - width 1: Supplementary Private Use Area-B (U+100000…);
/// - width 2: unassigned CJK-wide Plane 3 (U+3D000…);
/// - width 0: Variation Selectors Supplement (U+E0100…), which also stands in
///   for U+FE0F always — vt100 WIDENS the cell an emoji-presentation selector
///   follows, xterm never does.
///
/// A genuine character inside any stand-in range is itself stood in for, so
/// the screen never holds an ambiguous code point. Each width is measured on
/// vt100 itself (a one-row probe), so the ranges are checked, not assumed.
#[derive(Default)]
struct Proxies {
    forward: HashMap<char, char>,
    back: HashMap<char, char>,
    next: [u32; 3],
    vt_widths: HashMap<char, u8>,
}

const PROXY_RANGES: [(u32, u32); 3] =
    [(0xE0100, 0xE01EF), (0x100000, 0x10FFFD), (0x3D000, 0x3FFFD)];

impl Proxies {
    fn in_proxy_range(c: char) -> bool {
        let cp = c as u32;
        PROXY_RANGES
            .iter()
            .any(|(lo, hi)| (*lo..=*hi).contains(&cp))
    }

    /// vt100's width for `c`: 0 when it draws nothing of its own.
    fn vt_width(&mut self, c: char) -> u8 {
        *self.vt_widths.entry(c).or_insert_with(|| {
            let mut probe = vt100::Parser::new(
                NonZeroU16::MIN,
                NonZeroU16::new(8).unwrap_or(NonZeroU16::MIN),
                0,
            );
            let mut s = String::from("a");
            s.push(c);
            probe.process(s.as_bytes());
            probe.screen().cursor_position().1.saturating_sub(1) as u8
        })
    }

    /// The character to feed vt100 for `c`, of xterm width `width`.
    fn map(&mut self, c: char, width: u8) -> char {
        if let Some(p) = self.forward.get(&c) {
            return *p;
        }
        if c != '\u{fe0f}' && !Self::in_proxy_range(c) && self.vt_width(c) == width {
            return c;
        }
        let slot = usize::from(width.min(2));
        let (lo, hi) = PROXY_RANGES[slot];
        loop {
            let cp = lo + self.next[slot];
            if cp > hi {
                // Out of stand-ins: feed the character as it is.
                return c;
            }
            self.next[slot] += 1;
            let Some(p) = char::from_u32(cp) else {
                continue;
            };
            if self.back.contains_key(&p) || self.vt_width(p) != width {
                continue;
            }
            self.forward.insert(c, p);
            self.back.insert(p, c);
            return p;
        }
    }

    /// Append `contents` with every stand-in mapped back.
    fn unmap_into(&self, contents: &str, out: &mut String) {
        if self.back.is_empty() {
            out.push_str(contents);
            return;
        }
        out.extend(contents.chars().map(|c| *self.back.get(&c).unwrap_or(&c)));
    }
}

/// xterm.js's `UnicodeV6.wcwidth` (the default Unicode version of
/// `@xterm/headless` 6.0.0), verbatim: controls 0, the Unicode 6 combining
/// ranges 0, the East Asian wide ranges 2, everything else 1.
fn xterm_wcwidth(cp: u32) -> u8 {
    if cp < 32 {
        return 0;
    }
    if cp < 127 {
        return 1;
    }
    if cp < 0x10000 {
        if (127..160).contains(&cp) || in_ranges(cp, COMBINING_BMP) {
            return 0;
        }
        let wide = (0x1100..0x1160).contains(&cp)
            || cp == 0x2329
            || cp == 0x232A
            || ((0x2E80..0xA4D0).contains(&cp) && cp != 0x303F)
            || (0xAC00..0xD7A4).contains(&cp)
            || (0xF900..0xFB00).contains(&cp)
            || (0xFE10..0xFE1A).contains(&cp)
            || (0xFE30..0xFE70).contains(&cp)
            || (0xFF00..0xFF61).contains(&cp)
            || (0xFFE0..0xFFE7).contains(&cp);
        return if wide { 2 } else { 1 };
    }
    if in_ranges(cp, COMBINING_HIGH) {
        return 0;
    }
    if (0x20000..=0x2FFFD).contains(&cp) || (0x30000..=0x3FFFD).contains(&cp) {
        2
    } else {
        1
    }
}

fn in_ranges(cp: u32, ranges: &[(u32, u32)]) -> bool {
    ranges
        .binary_search_by(|(lo, hi)| {
            if *hi < cp {
                std::cmp::Ordering::Less
            } else if *lo > cp {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// xterm.js `UnicodeV6`'s BMP combining ranges (width 0), copied from
/// `@xterm/headless` 6.0.0 at the pin.
const COMBINING_BMP: &[(u32, u32)] = &[
    (0x0300, 0x036F),
    (0x0483, 0x0486),
    (0x0488, 0x0489),
    (0x0591, 0x05BD),
    (0x05BF, 0x05BF),
    (0x05C1, 0x05C2),
    (0x05C4, 0x05C5),
    (0x05C7, 0x05C7),
    (0x0600, 0x0603),
    (0x0610, 0x0615),
    (0x064B, 0x065E),
    (0x0670, 0x0670),
    (0x06D6, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x070F, 0x070F),
    (0x0711, 0x0711),
    (0x0730, 0x074A),
    (0x07A6, 0x07B0),
    (0x07EB, 0x07F3),
    (0x0901, 0x0902),
    (0x093C, 0x093C),
    (0x0941, 0x0948),
    (0x094D, 0x094D),
    (0x0951, 0x0954),
    (0x0962, 0x0963),
    (0x0981, 0x0981),
    (0x09BC, 0x09BC),
    (0x09C1, 0x09C4),
    (0x09CD, 0x09CD),
    (0x09E2, 0x09E3),
    (0x0A01, 0x0A02),
    (0x0A3C, 0x0A3C),
    (0x0A41, 0x0A42),
    (0x0A47, 0x0A48),
    (0x0A4B, 0x0A4D),
    (0x0A70, 0x0A71),
    (0x0A81, 0x0A82),
    (0x0ABC, 0x0ABC),
    (0x0AC1, 0x0AC5),
    (0x0AC7, 0x0AC8),
    (0x0ACD, 0x0ACD),
    (0x0AE2, 0x0AE3),
    (0x0B01, 0x0B01),
    (0x0B3C, 0x0B3C),
    (0x0B3F, 0x0B3F),
    (0x0B41, 0x0B43),
    (0x0B4D, 0x0B4D),
    (0x0B56, 0x0B56),
    (0x0B82, 0x0B82),
    (0x0BC0, 0x0BC0),
    (0x0BCD, 0x0BCD),
    (0x0C3E, 0x0C40),
    (0x0C46, 0x0C48),
    (0x0C4A, 0x0C4D),
    (0x0C55, 0x0C56),
    (0x0CBC, 0x0CBC),
    (0x0CBF, 0x0CBF),
    (0x0CC6, 0x0CC6),
    (0x0CCC, 0x0CCD),
    (0x0CE2, 0x0CE3),
    (0x0D41, 0x0D43),
    (0x0D4D, 0x0D4D),
    (0x0DCA, 0x0DCA),
    (0x0DD2, 0x0DD4),
    (0x0DD6, 0x0DD6),
    (0x0E31, 0x0E31),
    (0x0E34, 0x0E3A),
    (0x0E47, 0x0E4E),
    (0x0EB1, 0x0EB1),
    (0x0EB4, 0x0EB9),
    (0x0EBB, 0x0EBC),
    (0x0EC8, 0x0ECD),
    (0x0F18, 0x0F19),
    (0x0F35, 0x0F35),
    (0x0F37, 0x0F37),
    (0x0F39, 0x0F39),
    (0x0F71, 0x0F7E),
    (0x0F80, 0x0F84),
    (0x0F86, 0x0F87),
    (0x0F90, 0x0F97),
    (0x0F99, 0x0FBC),
    (0x0FC6, 0x0FC6),
    (0x102D, 0x1030),
    (0x1032, 0x1032),
    (0x1036, 0x1037),
    (0x1039, 0x1039),
    (0x1058, 0x1059),
    (0x1160, 0x11FF),
    (0x135F, 0x135F),
    (0x1712, 0x1714),
    (0x1732, 0x1734),
    (0x1752, 0x1753),
    (0x1772, 0x1773),
    (0x17B4, 0x17B5),
    (0x17B7, 0x17BD),
    (0x17C6, 0x17C6),
    (0x17C9, 0x17D3),
    (0x17DD, 0x17DD),
    (0x180B, 0x180D),
    (0x18A9, 0x18A9),
    (0x1920, 0x1922),
    (0x1927, 0x1928),
    (0x1932, 0x1932),
    (0x1939, 0x193B),
    (0x1A17, 0x1A18),
    (0x1B00, 0x1B03),
    (0x1B34, 0x1B34),
    (0x1B36, 0x1B3A),
    (0x1B3C, 0x1B3C),
    (0x1B42, 0x1B42),
    (0x1B6B, 0x1B73),
    (0x1DC0, 0x1DCA),
    (0x1DFE, 0x1DFF),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2063),
    (0x206A, 0x206F),
    (0x20D0, 0x20EF),
    (0x302A, 0x302F),
    (0x3099, 0x309A),
    (0xA806, 0xA806),
    (0xA80B, 0xA80B),
    (0xA825, 0xA826),
    (0xFB1E, 0xFB1E),
    (0xFE00, 0xFE0F),
    (0xFE20, 0xFE23),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
];

/// xterm.js `UnicodeV6`'s astral combining ranges (width 0).
const COMBINING_HIGH: &[(u32, u32)] = &[
    (0x10A01, 0x10A03),
    (0x10A05, 0x10A06),
    (0x10A0C, 0x10A0F),
    (0x10A38, 0x10A3A),
    (0x10A3F, 0x10A3F),
    (0x1D167, 0x1D169),
    (0x1D173, 0x1D182),
    (0x1D185, 0x1D18B),
    (0x1D1AA, 0x1D1AD),
    (0x1D242, 0x1D244),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
    (0xE0100, 0xE01EF),
];

/// A server-side terminal emulator for one PTY session (v4 `TerminalScreen`).
pub struct TerminalScreen {
    parser: vt100::Parser<ScrollTracker>,
    scrollback: usize,
    /// Absolute normal-buffer line where the next plain-shell summary starts
    /// (`dropped + index`); disposed once that line leaves the scrollback.
    summary_marker: Option<u64>,
    /// Last line of the previous plain-shell summary, to avoid repeating it.
    last_summary_tail: Option<String>,
    /// Text of the previous full-screen snapshot, to avoid posting an
    /// unchanged screen.
    last_snapshot: Option<String>,
    /// Width-correct stand-ins for characters vt100 would size differently.
    proxies: Proxies,
    /// The tail of the previous write that might begin an escape sequence this
    /// screen intercepts (held back: an incomplete sequence draws nothing).
    carry: String,
}

fn clamp_size(cols: u16, rows: u16) -> (NonZeroU16, NonZeroU16) {
    let cols = NonZeroU16::new(cols.max(MINIMUM_COLS)).unwrap_or(NonZeroU16::MIN);
    let rows = NonZeroU16::new(rows.max(MINIMUM_ROWS)).unwrap_or(NonZeroU16::MIN);
    (cols, rows)
}

impl TerminalScreen {
    /// A screen of `cols` × `rows` keeping [`SCREEN_SCROLLBACK_LINES`] lines of
    /// scrollback.
    pub fn new(cols: u16, rows: u16) -> Self {
        Self::with_scrollback(cols, rows, SCREEN_SCROLLBACK_LINES)
    }

    /// v4's optional third constructor argument.
    pub fn with_scrollback(cols: u16, rows: u16, scrollback: usize) -> Self {
        let (cols, rows) = clamp_size(cols, rows);
        Self {
            // vt100 takes ROWS first.
            parser: vt100::Parser::new_with_callbacks(
                rows,
                cols,
                scrollback,
                ScrollTracker::default(),
            ),
            scrollback,
            summary_marker: None,
            last_summary_tail: None,
            last_snapshot: None,
            proxies: Proxies::default(),
            carry: String::new(),
        }
    }

    /// Feed a held-back incomplete escape as it stands (the end of a
    /// transcript: nothing more is coming).
    fn flush_carry(&mut self) {
        let carry = std::mem::take(&mut self.carry);
        if !carry.is_empty() {
            self.parser.process(carry.as_bytes());
        }
    }

    pub fn cols(&self) -> u16 {
        self.parser.screen().size().1.get()
    }

    pub fn rows(&self) -> u16 {
        self.parser.screen().size().0.get()
    }

    pub fn write(&mut self, data: &str) {
        let mut input = std::mem::take(&mut self.carry);
        input.push_str(data);
        let input = normalize_controls(&input);
        let chars: Vec<char> = input.chars().collect();
        let mut seg = String::new();
        let mut i = 0usize;
        while i < chars.len() {
            let c = chars[i];
            if c == '\u{1b}' {
                match scan_escape(&chars[i..]) {
                    Scan::Incomplete if chars.len() - i <= CARRY_LIMIT => {
                        self.carry = chars[i..].iter().collect();
                        break;
                    }
                    Scan::Csi {
                        len,
                        private,
                        params,
                        intermediates,
                        final_byte,
                    } if intermediates.is_empty() => {
                        let seq: String = chars[i..i + len].iter().collect();
                        let action = classify_csi(private, &params, final_byte);
                        if action.is_some() {
                            self.flush(&mut seg);
                        }
                        match action {
                            Some(Intercept::EraseInDisplay(mode)) => {
                                self.erase_in_display(mode, &seq)
                            }
                            Some(Intercept::AltScreen { mode, set }) => self.alt_screen(mode, set),
                            None => seg.push_str(&seq),
                        }
                        i += len;
                        continue;
                    }
                    _ => {
                        seg.push(c);
                        i += 1;
                        continue;
                    }
                }
            }
            if (c as u32) < 0x80 {
                seg.push(c);
            } else {
                let width = xterm_wcwidth(c as u32);
                let mapped = self.proxies.map(c, width);
                if width == 2 {
                    self.flush(&mut seg);
                    self.prepare_wide_at_edge();
                }
                seg.push(mapped);
            }
            i += 1;
        }
        self.flush(&mut seg);
    }

    fn flush(&mut self, seg: &mut String) {
        if !seg.is_empty() {
            self.parser.process(seg.as_bytes());
            seg.clear();
        }
    }

    /// xterm wraps a wide character that does not fit in the last column and
    /// flags the next row a continuation even though the last cell stays
    /// empty; vt100 flags the wrap only when the last cell has content (tmux's
    /// rule). A blank written into that last cell first gives vt100 the same
    /// wrap — text-identical, since a continued row renders every cell, blank
    /// or space, as a space, and a trailing space is trimmed everywhere else.
    fn prepare_wide_at_edge(&mut self) {
        let screen = self.parser.screen();
        let (_, cols) = screen.size();
        let last = cols.get() - 1;
        let (row, col) = screen.cursor_position();
        if col == last && screen.cell(row, last).is_some_and(|c| !c.has_contents()) {
            self.parser.process(b" ");
        }
    }

    /// `CSI [?] Ps J` — xterm's `eraseInDisplay` resets whole lines through
    /// `_resetBufferLine`, which DISPOSES any marker on them (`clearMarkers`):
    /// mode 0 the rows below the cursor, mode 1 the rows above it, mode 2
    /// every row (`scrollOnEraseInDisplay` is off). The summary marker lives
    /// on the normal buffer, so only a normal-buffer erase can reach it.
    fn erase_in_display(&mut self, mode: u16, seq: &str) {
        let screen = self.parser.screen();
        if !screen.alternate_screen() {
            if let Some(row) = self.marker_screen_row() {
                let cursor = screen.cursor_position().0;
                let rows = screen.size().0.get();
                let reset = match mode {
                    0 => row > cursor && row < rows,
                    1 => row < cursor,
                    2 => row < rows,
                    _ => false,
                };
                if reset {
                    self.summary_marker = None;
                }
            }
        }
        self.parser.process(seq.as_bytes());
    }

    /// The summary marker's row on the visible screen, if it is there.
    fn marker_screen_row(&self) -> Option<u16> {
        let abs = self.summary_marker?;
        let dropped = self.parser.callbacks().dropped;
        let idx = abs.checked_sub(dropped)?;
        let held = self.parser.screen().scrollback_len() as u64;
        u16::try_from(idx.checked_sub(held)?).ok()
    }

    /// `CSI ? 47 | 1047 | 1049 h | l`, as xterm runs them (vt100 homes the
    /// cursor and clears the alternate screen on every `?1049h`, and keeps each
    /// buffer's own cursor across a switch). xterm: `?1049h` saves the cursor
    /// on the ACTIVE buffer; entering the alternate screen (only from the
    /// normal one — a repeat is a no-op) blanks it and carries the normal
    /// cursor over; leaving it carries the alternate cursor back and clears
    /// the alternate buffer (its scroll region included); `?1049l` then
    /// restores the saved cursor.
    fn alt_screen(&mut self, mode: u16, set: bool) {
        let alternate = self.parser.screen().alternate_screen();
        if set {
            if mode == 1049 {
                self.parser.process(b"\x1b7");
            }
            if alternate {
                return;
            }
            let (row, col) = self.parser.screen().cursor_position();
            self.parser.process(b"\x1b[?47h\x1b[2J");
            self.parser
                .process(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
        } else {
            if alternate {
                let (row, col) = self.parser.screen().cursor_position();
                self.parser.process(b"\x1b[r\x1b[?47l");
                self.parser
                    .process(format!("\x1b[{};{}H", row + 1, col + 1).as_bytes());
            }
            if mode == 1049 {
                self.parser.process(b"\x1b8");
            }
        }
    }

    /// Resize the emulator (xterm's clamp; no reflow — see the module doc).
    pub fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = clamp_size(cols, rows);
        let (cur_rows, cur_cols) = self.parser.screen().size();
        if cur_rows == rows && cur_cols == cols {
            return;
        }
        self.parser.set_size(rows, cols);
    }

    /// The active buffer's lines with xterm's wrap view of them.
    fn snapshot(&self) -> (ScreenBufferKind, BufferSnapshot) {
        let screen = self.parser.screen();
        let (rows, cols) = screen.size();
        let (rows, cols) = (rows.get(), cols.get());
        let tracker = self.parser.callbacks();
        if screen.alternate_screen() {
            let lines = (0..rows)
                .map(|r| render_row(screen, r, cols, &self.proxies).0)
                .collect();
            let wraps = (0..rows).map(|r| screen.row_wrapped(r)).collect();
            return (
                ScreenBufferKind::Alternate,
                BufferSnapshot {
                    lines,
                    wraps_into_next: wraps,
                    first_is_continuation: false,
                    full: false,
                },
            );
        }
        let held = screen.scrollback_len();
        let mut lines: Vec<LineText> = Vec::with_capacity(held + usize::from(rows));
        let mut wraps: Vec<bool> = Vec::with_capacity(held + usize::from(rows));
        if held > 0 {
            // Read the scrollback's cells from a clone (viewing mutates the
            // rows); its wrap flags from the untouched original.
            let mut view = screen.clone();
            let mut basics: Vec<String> = Vec::with_capacity(held);
            let mut y = 0usize;
            while y < held {
                view.set_scrollback(held - y);
                let take = usize::from(rows).min(held - y);
                for r in 0..take {
                    let (line, basic) = render_row(&view, r as u16, cols, &self.proxies);
                    lines.push(line);
                    basics.push(basic);
                }
                y += take;
            }
            match scrollback_wrap_flags(screen, &basics) {
                Some(flags) => wraps.extend(flags),
                None => {
                    debug_assert!(false, "vt100 basic capture did not parse");
                    view.set_scrollback(0);
                    wraps.extend(std::iter::repeat_n(false, held));
                }
            }
        }
        for r in 0..rows {
            lines.push(render_row(screen, r, cols, &self.proxies).0);
            wraps.push(screen.row_wrapped(r));
        }
        let full = held >= self.scrollback;
        (
            ScreenBufferKind::Normal,
            BufferSnapshot {
                lines,
                wraps_into_next: wraps,
                first_is_continuation: tracker.top_is_continuation,
                full,
            },
        )
    }

    /// The screen as it stands.
    pub fn render(&self) -> ScreenRendering {
        let (buffer, snap) = self.snapshot();
        ScreenRendering {
            buffer,
            lines: snap.read_lines(0, snap.lines.len(), buffer == ScreenBufferKind::Normal),
            cols: self.cols(),
            rows: self.rows(),
        }
    }

    /// What Ariel should post now. A full-screen program gets a snapshot of its
    /// screen (skipped when unchanged since the last one); a plain shell gets
    /// the lines written since the previous summary, starting from the line the
    /// cursor was on then — so a typed command shows up beside its output.
    pub fn take_summary(&mut self) -> Option<ScreenSummary> {
        let (buffer, snap) = self.snapshot();
        if buffer == ScreenBufferKind::Alternate {
            let text = snap.read_lines(0, snap.lines.len(), false).join("\n");
            if js_trim(&text).is_empty() || self.last_snapshot.as_deref() == Some(text.as_str()) {
                return None;
            }
            self.last_snapshot = Some(text.clone());
            return Some(ScreenSummary {
                kind: SummaryKind::Screen,
                text,
            });
        }
        self.last_snapshot = None;

        // A marker scrolled out of the scrollback is disposed; start from the
        // oldest line still held.
        let dropped = self.parser.callbacks().dropped;
        let from = match self.summary_marker {
            Some(abs) if abs >= dropped => usize::try_from(abs - dropped).unwrap_or(usize::MAX),
            _ => 0,
        };
        let mut lines = snap.read_lines(from, snap.lines.len(), true);
        if !lines.is_empty()
            && self
                .last_summary_tail
                .as_deref()
                .is_some_and(|tail| lines[0] == tail)
        {
            lines.remove(0);
        }

        // Registered BEFORE the null decision: it advances even over an
        // all-blank window.
        let screen = self.parser.screen();
        let cursor_row = usize::from(screen.cursor_position().0);
        self.summary_marker = Some(dropped + (screen.scrollback_len() + cursor_row) as u64);

        if lines.is_empty() || js_trim(&lines.concat()).is_empty() {
            return None;
        }
        self.last_summary_tail = lines.last().cloned();
        Some(ScreenSummary {
            kind: SummaryKind::Output,
            text: lines.join("\n"),
        })
    }
}

/// Render a recorded byte stream (a transcript tail) at the given geometry.
/// Used for exited sessions, whose live emulator is gone. A tail that starts
/// mid-sequence or after a full-screen program entered the alternate screen
/// renders approximately; the final screen is what it gets right.
pub fn render_transcript(raw: &str, cols: u16, rows: u16) -> ScreenRendering {
    let mut screen = TerminalScreen::new(cols, rows);
    screen.write(raw);
    screen.flush_carry();
    screen.render()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ESC: &str = "\u{1b}";

    fn summary(kind: SummaryKind, text: &str) -> Option<ScreenSummary> {
        Some(ScreenSummary {
            kind,
            text: text.to_string(),
        })
    }

    // v4's ten `screen.test.ts` cases, as cheap insurance — the proof is the
    // tier-1 family `terminal_screen_equivalence`.

    #[test]
    fn renders_a_plain_shell_as_scrollback_lines() {
        let mut s = TerminalScreen::new(40, 5);
        s.write("$ echo hi\r\nhi\r\n$ ");
        let r = s.render();
        assert_eq!(r.buffer, ScreenBufferKind::Normal);
        assert_eq!(r.lines, ["$ echo hi", "hi", "$"]);
    }

    #[test]
    fn keeps_lines_that_scrolled_off_the_visible_rows() {
        let mut s = TerminalScreen::new(40, 3);
        s.write(&["one", "two", "three", "four", "five"].join("\r\n"));
        assert_eq!(s.render().lines, ["one", "two", "three", "four", "five"]);
    }

    #[test]
    fn applies_cursor_addressing_the_way_a_full_screen_program_draws() {
        let mut s = TerminalScreen::new(40, 6);
        s.write(&format!("{ESC}[?1049h{ESC}[H{ESC}[2J"));
        s.write(&format!("{ESC}[1;1HHello{ESC}[1Cworld"));
        s.write(&format!("{ESC}[3;3H{ESC}[38;5;174m⏺{ESC}[39m Fixing it."));
        s.write(&format!("{ESC}[5;1Hstale{ESC}[5;1H{ESC}[Kfresh"));
        let r = s.render();
        assert_eq!(r.buffer, ScreenBufferKind::Alternate);
        assert_eq!(r.lines, ["Hello world", "", "  ⏺ Fixing it.", "", "fresh"]);
    }

    #[test]
    fn joins_soft_wrapped_rows_back_into_one_line() {
        let mut s = TerminalScreen::new(10, 5);
        s.write("abcdefghijKLMNO\r\nnext");
        assert_eq!(s.render().lines, ["abcdefghijKLMNO", "next"]);
    }

    #[test]
    fn follows_a_resize() {
        let mut s = TerminalScreen::new(80, 24);
        s.resize(100, 30);
        let r = s.render();
        assert_eq!((r.cols, r.rows), (100, 30));
    }

    #[test]
    fn summary_returns_output_since_the_previous_without_repeating_its_last_line() {
        let mut s = TerminalScreen::new(40, 10);
        s.write("$ ls\r\na.txt\r\n$ ");
        assert_eq!(
            s.take_summary(),
            summary(SummaryKind::Output, "$ ls\na.txt\n$")
        );
        s.write("pwd\r\n/tmp\r\n$ ");
        assert_eq!(
            s.take_summary(),
            summary(SummaryKind::Output, "$ pwd\n/tmp\n$")
        );
    }

    #[test]
    fn summary_is_none_when_nothing_new_was_written() {
        let mut s = TerminalScreen::new(40, 10);
        s.write("$ ");
        let _ = s.take_summary();
        assert_eq!(s.take_summary(), None);
    }

    #[test]
    fn summary_snapshots_a_full_screen_program_skipping_an_unchanged_screen() {
        let mut s = TerminalScreen::new(40, 4);
        s.write(&format!(
            "{ESC}[?1049h{ESC}[H{ESC}[2JClaude{ESC}[2;1H> prompt"
        ));
        assert_eq!(
            s.take_summary(),
            summary(SummaryKind::Screen, "Claude\n> prompt")
        );
        // A cursor blink repaints nothing visible.
        s.write(&format!("{ESC}[?25l{ESC}[?25h"));
        assert_eq!(s.take_summary(), None);
        s.write(&format!("{ESC}[2;3Hdone  "));
        assert_eq!(
            s.take_summary(),
            summary(SummaryKind::Screen, "Claude\n> done")
        );
    }

    #[test]
    fn summary_returns_to_shell_output_when_the_program_leaves_the_alternate_screen() {
        let mut s = TerminalScreen::new(40, 6);
        s.write("$ ");
        let _ = s.take_summary();
        s.write(&format!("claude\r\n{ESC}[?1049h{ESC}[HTUI{ESC}[?1049l$ "));
        let got = s.take_summary().expect("a summary");
        assert_eq!(got.kind, SummaryKind::Output);
        assert!(got.text.contains("$ claude"));
        assert!(!got.text.contains("TUI"));
    }

    #[test]
    fn render_transcript_replays_a_recorded_stream_at_the_given_geometry() {
        let r = render_transcript(
            &format!("{ESC}[?1049h{ESC}[H{ESC}[2Jtop{ESC}[3;2Hbottom"),
            20,
            4,
        );
        assert_eq!(
            r,
            ScreenRendering {
                buffer: ScreenBufferKind::Alternate,
                lines: vec!["top".into(), "".into(), " bottom".into()],
                cols: 20,
                rows: 4,
            }
        );
    }

    /// The `[TerminalScreen] Failed to resize emulator` WARN has no v5 home:
    /// every `u16` pair resizes, clamped as xterm clamps.
    #[test]
    fn resize_never_fails() {
        let mut s = TerminalScreen::new(0, 0);
        assert_eq!((s.cols(), s.rows()), (2, 1));
        for (c, r) in [(0, 0), (1, 1), (u16::MAX, 1), (3, 500), (80, 24)] {
            s.write("abc\r\n");
            s.resize(c, r);
            assert_eq!((s.cols(), s.rows()), (c.max(2), r.max(1)));
            let _ = s.render();
        }
    }

    #[test]
    fn serialises_in_v4_key_order() {
        let r = render_transcript("$ ", 20, 4);
        assert_eq!(
            serde_json::to_string(&r).unwrap(),
            r#"{"buffer":"normal","lines":["$"],"cols":20,"rows":4}"#
        );
        assert_eq!(
            serde_json::to_string(&summary(SummaryKind::Screen, "x")).unwrap(),
            r#"{"kind":"screen","text":"x"}"#
        );
    }
}
