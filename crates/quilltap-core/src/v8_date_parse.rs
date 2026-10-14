//! V8's `Date.parse`, ported whole — the ES5 ISO date-time pass AND the
//! legacy (Safari-compatible) fallback — from V8 13.6.233.17, the engine in
//! Node 24.13.1 that runs v4 (`src/date/dateparser.h`, `dateparser-inl.h`,
//! `dateparser.cc`, and `ParseDateTimeString` / `MakeDay` / `MakeTime` /
//! `TryTimeClip` in `src/date/date.{h,cc}`).
//!
//! v4 calls `Date.parse` on stored and model-emitted timestamps everywhere
//! (an anchor line's `occurredAt`, the date guard, a fold's `earliest`, the
//! event clocks). The ISO-only port this replaces answered `None` for every
//! string only V8's legacy parser reads (`July 14, 2026`, `2026/07/14`,
//! `2026-7-4`, `Tue Jul 14 2026`, `2026-07-14 12:00`, full-width digits as
//! `07-14` of 2001) and for the expanded `+010000-…` year — the P4.D289
//! finding `JS_DATE_PARSE_ISO_SUBSET`, which this closes (the human's ruling,
//! 2026-10-14: port V8's parser).
//!
//! The port follows V8 statement for statement, including its quirks: the
//! input is read as UTF-16 code units, a NUL unit ends it, any unit `>= 'A'`
//! that is not whitespace is a word character (so full-width digits and
//! surrogate halves make words), a word's first three units are lower-cased
//! by `| 0x20`, the day composer's field order is decided AFTER its slots are
//! padded (so `July 14` reads year 1 → 2001), two-digit years window to
//! 1950–2049 on the legacy path only, and the time-zone total is computed in
//! wrapping 32-bit unsigned arithmetic against the 32-bit Smi maximum (Node
//! builds V8 without pointer compression).
//!
//! **One recorded divergence: a string with no UTC offset is read as UTC.**
//! V8 converts such a time from the PROCESS's local zone; v5 core never reads
//! an ambient zone (`host_zone.rs`), so the local→UTC step is the identity —
//! the ruled `ZONELESS_PARSE_AS_UTC` (P4.D289 LR-10). Under `TZ=UTC` (every
//! oracle that drives `Date.parse`) the two agree exactly. V8's range check
//! for that path (`kMaxTimeBeforeUTCInMs`) is kept.

/// V8 `kNone` (`kMaxInt`).
const K_NONE: i64 = i32::MAX as i64;
/// V8 `kMaxSignificantDigits`.
const MAX_SIGNIFICANT_DIGITS: i64 = 9;
/// V8 `Smi::kMaxValue` with 32-bit Smis (no pointer compression).
const SMI_MAX: i64 = i32::MAX as i64;
const SMI_MIN: i64 = i32::MIN as i64;

const MS_PER_DAY: f64 = 86_400_000.0;
/// V8 `DateCache::kMaxTimeInMs`.
const MAX_TIME_IN_MS: f64 = 864_000_000.0 * 10_000_000.0;
/// V8 `DateCache::kMaxTimeBeforeUTCInMs` (`kMaxTimeInMs + kMsPerMonth`).
const MAX_TIME_BEFORE_UTC_IN_MS: f64 = MAX_TIME_IN_MS + MS_PER_DAY * 30.0;

/// V8 `DateParser::Between`.
fn between(x: i64, lo: i64, hi: i64) -> bool {
    lo <= x && x <= hi
}

/// V8 `IsWhiteSpace`: TAB, VT, FF, SP, NBSP, then (above 0xFF) Unicode `Zs`
/// and U+FEFF.
fn is_white_space(c: u32) -> bool {
    matches!(
        c,
        0x09 | 0x0B | 0x0C | 0x20 | 0xA0 | 0x1680 | 0x2000
            ..=0x200A | 0x202F | 0x205F | 0x3000 | 0xFEFF
    )
}

/// V8 `IsWhiteSpaceOrLineTerminator`: [`is_white_space`] plus LF, CR, LS, PS.
fn is_white_space_or_line_terminator(c: u32) -> bool {
    is_white_space(c) || matches!(c, 0x0A | 0x0D | 0x2028 | 0x2029)
}

/// V8 `DateParser::InputReader`.
struct InputReader<'a> {
    buffer: &'a [u32],
    index: usize,
    ch: u32,
}

impl<'a> InputReader<'a> {
    fn new(buffer: &'a [u32]) -> Self {
        let mut reader = InputReader {
            buffer,
            index: 0,
            ch: 0,
        };
        reader.next();
        reader
    }

    fn position(&self) -> i64 {
        self.index as i64
    }

    fn next(&mut self) {
        self.ch = self.buffer.get(self.index).copied().unwrap_or(0);
        self.index += 1;
    }

    fn read_unsigned_numeral(&mut self) -> i64 {
        let mut n = 0i64;
        let mut i = 0i64;
        while self.ch == u32::from(b'0') {
            self.next();
        }
        while self.is_ascii_digit() {
            if i < MAX_SIGNIFICANT_DIGITS {
                n = n * 10 + i64::from(self.ch - u32::from(b'0'));
            }
            i += 1;
            self.next();
        }
        n
    }

    fn read_word(&mut self, prefix: &mut [u32; 3]) -> i64 {
        let mut len = 0usize;
        while self.is_ascii_alpha_or_above() && !self.is_white_space_char() {
            if len < 3 {
                // V8 `AsciiAlphaToLower(c)` is `c | 0x20` for every unit.
                prefix[len] = self.ch | 0x20;
            }
            self.next();
            len += 1;
        }
        for slot in prefix.iter_mut().skip(len) {
            *slot = 0;
        }
        len as i64
    }

    fn skip(&mut self, c: u8) -> bool {
        if self.ch == u32::from(c) {
            self.next();
            true
        } else {
            false
        }
    }

    fn skip_white_space(&mut self) -> bool {
        if is_white_space_or_line_terminator(self.ch) {
            self.next();
            true
        } else {
            false
        }
    }

    fn skip_parentheses(&mut self) -> bool {
        if self.ch != u32::from(b'(') {
            return false;
        }
        let mut balance = 0i64;
        loop {
            if self.ch == u32::from(b')') {
                balance -= 1;
            } else if self.ch == u32::from(b'(') {
                balance += 1;
            }
            self.next();
            if !(balance > 0 && self.ch != 0) {
                break;
            }
        }
        true
    }

    fn is_end(&self) -> bool {
        self.ch == 0
    }

    fn is_ascii_digit(&self) -> bool {
        (u32::from(b'0')..=u32::from(b'9')).contains(&self.ch)
    }

    fn is_ascii_alpha_or_above(&self) -> bool {
        self.ch >= u32::from(b'A')
    }

    fn is_white_space_char(&self) -> bool {
        is_white_space(self.ch)
    }
}

/// V8 `DateParser::KeywordType`.
const INVALID: i64 = 0;
const MONTH_NAME: i64 = 1;
const TIME_ZONE_NAME: i64 = 2;
const TIME_SEPARATOR: i64 = 3;
const AM_PM: i64 = 4;

/// V8 `DateToken`'s tags (a keyword's tag is its `KeywordType`, `>= 0`).
const INVALID_TOKEN_TAG: i64 = -6;
const UNKNOWN_TOKEN_TAG: i64 = -5;
const WHITE_SPACE_TAG: i64 = -4;
const NUMBER_TAG: i64 = -3;
const SYMBOL_TAG: i64 = -2;
const END_OF_INPUT_TAG: i64 = -1;

/// V8 `DateParser::DateToken`.
#[derive(Clone, Copy)]
struct DateToken {
    tag: i64,
    length: i64,
    value: i64,
}

impl DateToken {
    fn is_invalid(&self) -> bool {
        self.tag == INVALID_TOKEN_TAG
    }
    fn is_number(&self) -> bool {
        self.tag == NUMBER_TAG
    }
    fn is_symbol(&self) -> bool {
        self.tag == SYMBOL_TAG
    }
    fn is_white_space(&self) -> bool {
        self.tag == WHITE_SPACE_TAG
    }
    fn is_end_of_input(&self) -> bool {
        self.tag == END_OF_INPUT_TAG
    }
    fn is_keyword(&self) -> bool {
        self.tag >= 0
    }
    fn is_symbol_char(&self, symbol: u8) -> bool {
        self.is_symbol() && self.value == i64::from(symbol)
    }
    fn is_keyword_type(&self, keyword: i64) -> bool {
        self.tag == keyword
    }
    fn is_fixed_length_number(&self, length: i64) -> bool {
        self.is_number() && self.length == length
    }
    fn is_ascii_sign(&self) -> bool {
        self.tag == SYMBOL_TAG && (self.value == i64::from(b'-') || self.value == i64::from(b'+'))
    }
    /// `44 - value`: `+` is 1, `-` is -1.
    fn ascii_sign(&self) -> i64 {
        44 - self.value
    }
    fn is_keyword_z(&self) -> bool {
        self.is_keyword_type(TIME_ZONE_NAME) && self.length == 1 && self.value == 0
    }

    fn keyword(tag: i64, value: i64, length: i64) -> Self {
        DateToken { tag, length, value }
    }
    fn number(value: i64, length: i64) -> Self {
        DateToken {
            tag: NUMBER_TAG,
            length,
            value,
        }
    }
    fn symbol(symbol: u8) -> Self {
        DateToken {
            tag: SYMBOL_TAG,
            length: 1,
            value: i64::from(symbol),
        }
    }
    fn end_of_input() -> Self {
        DateToken {
            tag: END_OF_INPUT_TAG,
            length: 0,
            value: -1,
        }
    }
    fn white_space(length: i64) -> Self {
        DateToken {
            tag: WHITE_SPACE_TAG,
            length,
            value: -1,
        }
    }
    fn unknown() -> Self {
        DateToken {
            tag: UNKNOWN_TOKEN_TAG,
            length: 1,
            value: -1,
        }
    }
    fn invalid() -> Self {
        DateToken {
            tag: INVALID_TOKEN_TAG,
            length: 0,
            value: -1,
        }
    }
}

/// V8 `DateParser::KeywordTable::array` — `(prefix, type, value)`, ending in
/// the `INVALID` sentinel.
const KEYWORDS: [([u8; 3], i64, i64); 28] = [
    (*b"jan", MONTH_NAME, 1),
    (*b"feb", MONTH_NAME, 2),
    (*b"mar", MONTH_NAME, 3),
    (*b"apr", MONTH_NAME, 4),
    (*b"may", MONTH_NAME, 5),
    (*b"jun", MONTH_NAME, 6),
    (*b"jul", MONTH_NAME, 7),
    (*b"aug", MONTH_NAME, 8),
    (*b"sep", MONTH_NAME, 9),
    (*b"oct", MONTH_NAME, 10),
    (*b"nov", MONTH_NAME, 11),
    (*b"dec", MONTH_NAME, 12),
    (*b"am\0", AM_PM, 0),
    (*b"pm\0", AM_PM, 12),
    (*b"ut\0", TIME_ZONE_NAME, 0),
    (*b"utc", TIME_ZONE_NAME, 0),
    (*b"z\0\0", TIME_ZONE_NAME, 0),
    (*b"gmt", TIME_ZONE_NAME, 0),
    (*b"cdt", TIME_ZONE_NAME, -5),
    (*b"cst", TIME_ZONE_NAME, -6),
    (*b"edt", TIME_ZONE_NAME, -4),
    (*b"est", TIME_ZONE_NAME, -5),
    (*b"mdt", TIME_ZONE_NAME, -6),
    (*b"mst", TIME_ZONE_NAME, -7),
    (*b"pdt", TIME_ZONE_NAME, -7),
    (*b"pst", TIME_ZONE_NAME, -8),
    (*b"t\0\0", TIME_SEPARATOR, 0),
    (*b"\0\0\0", INVALID, 0),
];

/// V8 `KeywordTable::Lookup`: the first entry whose three-unit prefix
/// matches, where a word longer than three units may match a MONTH name only.
fn keyword_lookup(prefix: &[u32; 3], len: i64) -> usize {
    let mut i = 0;
    while KEYWORDS[i].1 != INVALID {
        let entry = &KEYWORDS[i];
        let mut j = 0;
        while j < 3 && prefix[j] == u32::from(entry.0[j]) {
            j += 1;
        }
        if j == 3 && (len <= 3 || entry.1 == MONTH_NAME) {
            return i;
        }
        i += 1;
    }
    i
}

/// V8 `DateParser::DateStringTokenizer`.
struct Tokenizer<'a> {
    input: InputReader<'a>,
    next: DateToken,
}

impl<'a> Tokenizer<'a> {
    fn new(input: InputReader<'a>) -> Self {
        let mut tokenizer = Tokenizer {
            input,
            next: DateToken::end_of_input(),
        };
        tokenizer.next = tokenizer.scan();
        tokenizer
    }

    fn next(&mut self) -> DateToken {
        let result = self.next;
        self.next = self.scan();
        result
    }

    fn peek(&self) -> DateToken {
        self.next
    }

    fn skip_symbol(&mut self, symbol: u8) -> bool {
        if self.next.is_symbol_char(symbol) {
            self.next = self.scan();
            true
        } else {
            false
        }
    }

    fn scan(&mut self) -> DateToken {
        let in_ = &mut self.input;
        let pre_pos = in_.position();
        if in_.is_end() {
            return DateToken::end_of_input();
        }
        if in_.is_ascii_digit() {
            let n = in_.read_unsigned_numeral();
            let length = in_.position() - pre_pos;
            return DateToken::number(n, length);
        }
        for symbol in [b':', b'-', b'+', b'.', b')'] {
            if in_.skip(symbol) {
                return DateToken::symbol(symbol);
            }
        }
        if in_.is_ascii_alpha_or_above() && !in_.is_white_space_char() {
            let mut prefix = [0u32; 3];
            let length = in_.read_word(&mut prefix);
            let index = keyword_lookup(&prefix, length);
            return DateToken::keyword(KEYWORDS[index].1, KEYWORDS[index].2, length);
        }
        if in_.skip_white_space() {
            return DateToken::white_space(in_.position() - pre_pos);
        }
        if in_.skip_parentheses() {
            return DateToken::unknown();
        }
        in_.next();
        DateToken::unknown()
    }
}

/// V8 `DateParser::ReadMilliseconds`: the first three significant digits of
/// the numeral, inferred from its value and its digit count.
fn read_milliseconds(token: DateToken) -> i64 {
    let mut number = token.value;
    let mut length = token.length;
    if length < 3 {
        if length == 1 {
            number *= 100;
        } else if length == 2 {
            number *= 10;
        }
    } else if length > 3 {
        if length > MAX_SIGNIFICANT_DIGITS {
            length = MAX_SIGNIFICANT_DIGITS;
        }
        let mut factor = 1i64;
        loop {
            factor *= 10;
            length -= 1;
            if length <= 3 {
                break;
            }
        }
        number /= factor;
    }
    number
}

/// V8 `TimeComposer::IsMinute` / `IsSecond` / `IsHour` / `IsHour12` /
/// `IsMillisecond`.
fn is_minute(x: i64) -> bool {
    between(x, 0, 59)
}
fn is_hour(x: i64) -> bool {
    between(x, 0, 23)
}
fn is_second(x: i64) -> bool {
    between(x, 0, 59)
}
fn is_hour12(x: i64) -> bool {
    between(x, 0, 12)
}
fn is_millisecond(x: i64) -> bool {
    between(x, 0, 999)
}
/// V8 `DayComposer::IsMonth` / `IsDay`.
fn is_month(x: i64) -> bool {
    between(x, 1, 12)
}
fn is_day(x: i64) -> bool {
    between(x, 1, 31)
}

/// V8 `DateParser::TimeZoneComposer`.
struct TimeZoneComposer {
    sign: i64,
    hour: i64,
    minute: i64,
}

impl TimeZoneComposer {
    fn new() -> Self {
        TimeZoneComposer {
            sign: K_NONE,
            hour: K_NONE,
            minute: K_NONE,
        }
    }
    fn set(&mut self, offset_in_hours: i64) {
        self.sign = if offset_in_hours < 0 { -1 } else { 1 };
        self.hour = offset_in_hours * self.sign;
        self.minute = 0;
    }
    fn set_sign(&mut self, sign: i64) {
        self.sign = if sign < 0 { -1 } else { 1 };
    }
    fn set_absolute_hour(&mut self, hour: i64) {
        self.hour = hour;
    }
    fn set_absolute_minute(&mut self, minute: i64) {
        self.minute = minute;
    }
    fn is_expecting(&self, n: i64) -> bool {
        self.hour != K_NONE && self.minute == K_NONE && is_minute(n)
    }
    fn is_utc(&self) -> bool {
        self.hour == 0 && self.minute == 0
    }
    fn is_empty(&self) -> bool {
        self.hour == K_NONE
    }
    /// The UTC offset in seconds, `Some(None)` for "no offset" (V8's NaN),
    /// `None` when the composer refuses.
    fn write(&mut self) -> Option<Option<i64>> {
        if self.sign == K_NONE {
            return Some(None);
        }
        if self.hour == K_NONE {
            self.hour = 0;
        }
        if self.minute == K_NONE {
            self.minute = 0;
        }
        // V8 avoids signed overflow with unsigned 32-bit arithmetic — which
        // WRAPS for an absurd hour, exactly as here.
        let total = (self.hour as u32)
            .wrapping_mul(3600)
            .wrapping_add((self.minute as u32).wrapping_mul(60));
        if i64::from(total) > SMI_MAX {
            return None;
        }
        let total = i64::from(total);
        Some(Some(if self.sign < 0 { -total } else { total }))
    }
}

/// V8 `DateParser::TimeComposer`.
struct TimeComposer {
    comp: [i64; 4],
    index: usize,
    hour_offset: i64,
}

impl TimeComposer {
    fn new() -> Self {
        TimeComposer {
            comp: [0; 4],
            index: 0,
            hour_offset: K_NONE,
        }
    }
    fn is_empty(&self) -> bool {
        self.index == 0
    }
    fn is_expecting(&self, n: i64) -> bool {
        (self.index == 1 && is_minute(n))
            || (self.index == 2 && is_second(n))
            || (self.index == 3 && is_millisecond(n))
    }
    fn add(&mut self, n: i64) -> bool {
        if self.index < 4 {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn add_final(&mut self, n: i64) -> bool {
        if !self.add(n) {
            return false;
        }
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        true
    }
    fn set_hour_offset(&mut self, n: i64) {
        self.hour_offset = n;
    }
    /// `[hour, minute, second, millisecond]`, or `None` when V8 refuses.
    fn write(&mut self) -> Option<[i64; 4]> {
        while self.index < 4 {
            self.comp[self.index] = 0;
            self.index += 1;
        }
        let [mut hour, minute, second, millisecond] = self.comp;
        if self.hour_offset != K_NONE {
            if !is_hour12(hour) {
                return None;
            }
            hour %= 12;
            hour += self.hour_offset;
        }
        if !is_hour(hour)
            || !is_minute(minute)
            || !is_second(second)
            || !is_millisecond(millisecond)
        {
            // A 24th hour is allowed if minutes, seconds and milliseconds are 0.
            if hour != 24 || minute != 0 || second != 0 || millisecond != 0 {
                return None;
            }
        }
        Some([hour, minute, second, millisecond])
    }
}

/// V8 `DateParser::DayComposer`.
struct DayComposer {
    comp: [i64; 3],
    index: usize,
    named_month: i64,
    is_iso_date: bool,
}

impl DayComposer {
    fn new() -> Self {
        DayComposer {
            comp: [0; 3],
            index: 0,
            named_month: K_NONE,
            is_iso_date: false,
        }
    }
    fn is_empty(&self) -> bool {
        self.index == 0
    }
    fn add(&mut self, n: i64) -> bool {
        if self.index < 3 {
            self.comp[self.index] = n;
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn set_named_month(&mut self, n: i64) {
        self.named_month = n;
    }
    fn set_iso_date(&mut self) {
        self.is_iso_date = true;
    }
    /// `[year, month (0-based), day]`, or `None` when V8 refuses.
    ///
    /// V8 pads the slots with 1 BEFORE it inspects `index_`, so every
    /// `index_ == 3` test is always true and the named-month `index_ == 1`
    /// branch never runs — `July 14` reads `[14, 1, 1]` and takes year 1
    /// (→ 2001). Ported as written.
    fn write(&mut self) -> Option<[i64; 3]> {
        if self.index < 1 {
            return None;
        }
        // Day and month default to 1.
        while self.index < 3 {
            self.comp[self.index] = 1;
            self.index += 1;
        }
        let mut year = 0i64; // Default year is 0 (=> 2000) for KJS compatibility.
        let month;
        let day;
        if self.named_month == K_NONE {
            if self.is_iso_date || (self.index == 3 && !is_day(self.comp[0])) {
                // YMD
                year = self.comp[0];
                month = self.comp[1];
                day = self.comp[2];
            } else {
                // MD(Y)
                month = self.comp[0];
                day = self.comp[1];
                if self.index == 3 {
                    year = self.comp[2];
                }
            }
        } else {
            month = self.named_month;
            if self.index == 1 {
                // MD or DM
                day = self.comp[0];
            } else if !is_day(self.comp[0]) {
                // YMD, MYD, or YDM
                year = self.comp[0];
                day = self.comp[1];
            } else {
                // DMY, MDY, or DYM
                day = self.comp[0];
                year = self.comp[1];
            }
        }
        if !self.is_iso_date {
            if between(year, 0, 49) {
                year += 2000;
            } else if between(year, 50, 99) {
                year += 1900;
            }
        }
        if !(SMI_MIN..=SMI_MAX).contains(&year) || !is_month(month) || !is_day(day) {
            return None;
        }
        Some([year, month - 1, day])
    }
}

/// V8 `DateParser::ParseES5DateTime`: as far as the ES5 ISO grammar goes,
/// returning the first token it did not handle (`EndOfInput` on a complete
/// ISO string, `Invalid` when the ISO time part is malformed).
fn parse_es5_date_time(
    scanner: &mut Tokenizer<'_>,
    day: &mut DayComposer,
    time: &mut TimeComposer,
    tz: &mut TimeZoneComposer,
) -> DateToken {
    // Parse mandatory date string: [('-'|'+')yy]yyyy[':'MM[':'DD]]
    if scanner.peek().is_ascii_sign() {
        // Keep the sign token, so we can pass it back to the legacy parser if
        // we don't use it.
        let sign_token = scanner.next();
        if !scanner.peek().is_fixed_length_number(6) {
            return sign_token;
        }
        let sign = sign_token.ascii_sign();
        let year = scanner.next().value;
        if sign < 0 && year == 0 {
            return sign_token;
        }
        day.add(sign * year);
    } else if scanner.peek().is_fixed_length_number(4) {
        day.add(scanner.next().value);
    } else {
        return scanner.next();
    }
    if scanner.skip_symbol(b'-') {
        if !scanner.peek().is_fixed_length_number(2) || !is_month(scanner.peek().value) {
            return scanner.next();
        }
        day.add(scanner.next().value);
        if scanner.skip_symbol(b'-') {
            if !scanner.peek().is_fixed_length_number(2) || !is_day(scanner.peek().value) {
                return scanner.next();
            }
            day.add(scanner.next().value);
        }
    }
    // Check for optional time string: 'T'HH':'mm[':'ss['.'sss]]Z
    if !scanner.peek().is_keyword_type(TIME_SEPARATOR) {
        if !scanner.peek().is_end_of_input() {
            return scanner.next();
        }
    } else {
        // ES5 Date Time String time part is present.
        scanner.next();
        if !scanner.peek().is_fixed_length_number(2) || !between(scanner.peek().value, 0, 24) {
            return DateToken::invalid();
        }
        // Allow 24:00[:00[.000]], but no other time starting with 24.
        let hour_is_24 = scanner.peek().value == 24;
        time.add(scanner.next().value);
        if !scanner.skip_symbol(b':') {
            return DateToken::invalid();
        }
        if !scanner.peek().is_fixed_length_number(2)
            || !is_minute(scanner.peek().value)
            || (hour_is_24 && scanner.peek().value > 0)
        {
            return DateToken::invalid();
        }
        time.add(scanner.next().value);
        if scanner.skip_symbol(b':') {
            if !scanner.peek().is_fixed_length_number(2)
                || !is_second(scanner.peek().value)
                || (hour_is_24 && scanner.peek().value > 0)
            {
                return DateToken::invalid();
            }
            time.add(scanner.next().value);
            if scanner.skip_symbol(b'.') {
                if !scanner.peek().is_number() || (hour_is_24 && scanner.peek().value > 0) {
                    return DateToken::invalid();
                }
                // Allow more or less than the mandated three digits.
                time.add(read_milliseconds(scanner.next()));
            }
        }
        // Check for optional timezone designation: 'Z' | ('+'|'-')hh':'mm
        if scanner.peek().is_keyword_z() {
            scanner.next();
            tz.set(0);
        } else if scanner.peek().is_symbol_char(b'+') || scanner.peek().is_symbol_char(b'-') {
            tz.set_sign(if scanner.next().value == i64::from(b'+') {
                1
            } else {
                -1
            });
            if scanner.peek().is_fixed_length_number(4) {
                // hhmm extension syntax.
                let hourmin = scanner.next().value;
                let hour = hourmin / 100;
                let min = hourmin % 100;
                if !is_hour(hour) || !is_minute(min) {
                    return DateToken::invalid();
                }
                tz.set_absolute_hour(hour);
                tz.set_absolute_minute(min);
            } else {
                // hh:mm standard syntax.
                if !scanner.peek().is_fixed_length_number(2) || !is_hour(scanner.peek().value) {
                    return DateToken::invalid();
                }
                tz.set_absolute_hour(scanner.next().value);
                if !scanner.skip_symbol(b':') {
                    return DateToken::invalid();
                }
                if !scanner.peek().is_fixed_length_number(2) || !is_minute(scanner.peek().value) {
                    return DateToken::invalid();
                }
                tz.set_absolute_minute(scanner.next().value);
            }
        }
        if !scanner.peek().is_end_of_input() {
            return DateToken::invalid();
        }
    }
    // Successfully parsed ES5 Date Time String. "When the time zone offset is
    // absent, date-only forms are interpreted as a UTC time and date-time
    // forms are interpreted as a local time."
    if tz.is_empty() && time.is_empty() {
        tz.set(0);
    }
    day.set_iso_date();
    DateToken::end_of_input()
}

/// V8 `DateParser::Parse` — `[year, month0, day, hour, minute, second, ms]`
/// plus the UTC offset in seconds (`None` = V8's NaN, "no offset").
fn parse_fields(units: &[u32]) -> Option<([i64; 7], Option<i64>)> {
    let mut scanner = Tokenizer::new(InputReader::new(units));
    let mut tz = TimeZoneComposer::new();
    let mut time = TimeComposer::new();
    let mut day = DayComposer::new();

    // First try getting as far as possible with as ES5 Date Time String.
    let next_unhandled_token = parse_es5_date_time(&mut scanner, &mut day, &mut time, &mut tz);
    if next_unhandled_token.is_invalid() {
        return None;
    }
    let mut has_read_number = !day.is_empty();
    // If there's anything left, continue with the legacy parser.
    let mut token = next_unhandled_token;
    while !token.is_end_of_input() {
        if token.is_number() {
            has_read_number = true;
            let n = token.value;
            if scanner.skip_symbol(b':') {
                if scanner.skip_symbol(b':') {
                    // n + "::"
                    if !time.is_empty() {
                        return None;
                    }
                    time.add(n);
                    time.add(0);
                } else {
                    // n + ":"
                    if !time.add(n) {
                        return None;
                    }
                    if scanner.peek().is_symbol_char(b'.') {
                        scanner.next();
                    }
                }
            } else if scanner.skip_symbol(b'.') && time.is_expecting(n) {
                time.add(n);
                if !scanner.peek().is_number() {
                    return None;
                }
                let ms = read_milliseconds(scanner.next());
                if ms < 0 {
                    return None;
                }
                time.add_final(ms);
            } else if tz.is_expecting(n) {
                tz.set_absolute_minute(n);
            } else if time.is_expecting(n) {
                time.add_final(n);
                // Require end, white space, "Z", "+" or "-" immediately after
                // finalizing time.
                let peek = scanner.peek();
                if !peek.is_end_of_input()
                    && !peek.is_white_space()
                    && !peek.is_keyword_z()
                    && !peek.is_ascii_sign()
                {
                    return None;
                }
            } else {
                if !day.add(n) {
                    return None;
                }
                scanner.skip_symbol(b'-');
            }
        } else if token.is_keyword() {
            // Parse a "word" (sequence of chars. >= 'A').
            let kind = token.tag;
            let value = token.value;
            if kind == AM_PM && !time.is_empty() {
                time.set_hour_offset(value);
            } else if kind == MONTH_NAME {
                day.set_named_month(value);
                scanner.skip_symbol(b'-');
            } else if kind == TIME_ZONE_NAME && has_read_number {
                tz.set(value);
            } else {
                // Garbage words are illegal if a number has been read.
                if has_read_number {
                    return None;
                }
                // The first number has to be separated from garbage words by
                // whitespace or other separators.
                if scanner.peek().is_number() {
                    return None;
                }
            }
        } else if token.is_ascii_sign() && (tz.is_utc() || !time.is_empty()) {
            // Parse UTC offset (only after UTC or time).
            tz.set_sign(token.ascii_sign());
            // The following number may be empty.
            let mut n = 0i64;
            let mut length = 0i64;
            if scanner.peek().is_number() {
                let next_token = scanner.next();
                length = next_token.length;
                n = next_token.value;
            }
            has_read_number = true;

            if scanner.peek().is_symbol_char(b':') {
                tz.set_absolute_hour(n);
                tz.set_absolute_minute(K_NONE);
            } else if length == 2 || length == 1 {
                // Handle time zones like GMT-8
                tz.set_absolute_hour(n);
                tz.set_absolute_minute(0);
            } else if length == 4 || length == 3 {
                // Looks like the hhmm format
                tz.set_absolute_hour(n / 100);
                tz.set_absolute_minute(n % 100);
            } else {
                // No need to accept time zones like GMT-12345
                return None;
            }
        } else if (token.is_ascii_sign() || token.is_symbol_char(b')')) && has_read_number {
            // Extra sign or ')' is illegal if a number has been read.
            return None;
        }
        // Ignore other characters and whitespace.
        token = scanner.next();
    }

    let [year, month, d] = day.write()?;
    let [hour, minute, second, millisecond] = time.write()?;
    let offset = tz.write()?;
    Some(([year, month, d, hour, minute, second, millisecond], offset))
}

/// V8 `MakeDay` (its own integer day-from-year arithmetic, kept literally).
fn make_day(year: f64, month: f64, date: f64) -> f64 {
    const MIN_YEAR: f64 = -1_000_000.0;
    const MAX_YEAR: f64 = 1_000_000.0;
    const MIN_MONTH: f64 = -10_000_000.0;
    const MAX_MONTH: f64 = 10_000_000.0;
    let in_range = (MIN_YEAR..=MAX_YEAR).contains(&year)
        && (MIN_MONTH..=MAX_MONTH).contains(&month)
        && date.is_finite();
    if !in_range {
        return f64::NAN;
    }
    let mut y = year as i64;
    let mut m = month as i64;
    y += m / 12;
    m %= 12;
    if m < 0 {
        m += 12;
        y -= 1;
    }
    const YEAR_DELTA: i64 = 399_999;
    const BASE_DAY: i64 = 365 * (1970 + YEAR_DELTA) + (1970 + YEAR_DELTA) / 4
        - (1970 + YEAR_DELTA) / 100
        + (1970 + YEAR_DELTA) / 400;
    let mut day_from_year = 365 * (y + YEAR_DELTA) + (y + YEAR_DELTA) / 4 - (y + YEAR_DELTA) / 100
        + (y + YEAR_DELTA) / 400
        - BASE_DAY;
    const COMMON: [i64; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    const LEAP: [i64; 12] = [0, 31, 60, 91, 121, 152, 182, 213, 244, 274, 305, 335];
    if (y % 4 != 0) || (y % 100 == 0 && y % 400 != 0) {
        day_from_year += COMMON[m as usize];
    } else {
        day_from_year += LEAP[m as usize];
    }
    (day_from_year - 1) as f64 + date.trunc()
}

/// JS `Date.parse(s)` as V8 13.6 computes it (see the module doc), as integer
/// epoch milliseconds; `None` where V8 answers NaN.
pub(crate) fn date_parse_ms(s: &str) -> Option<i64> {
    let units: Vec<u32> = s.encode_utf16().map(u32::from).collect();
    let ([year, month, d, hour, minute, second, millisecond], offset) = parse_fields(&units)?;
    let day = make_day(year as f64, month as f64, d as f64);
    let time = hour as f64 * 3_600_000.0
        + minute as f64 * 60_000.0
        + second as f64 * 1000.0
        + millisecond as f64;
    if !day.is_finite() {
        return None;
    }
    let mut date = time + day * MS_PER_DAY;
    match offset {
        None => {
            if !(-MAX_TIME_BEFORE_UTC_IN_MS..=MAX_TIME_BEFORE_UTC_IN_MS).contains(&date) {
                return None;
            }
            // ZONELESS_PARSE_AS_UTC: V8's local→UTC step is the identity here.
        }
        Some(seconds) => date -= seconds as f64 * 1000.0,
    }
    // V8 `TryTimeClip`.
    if !(-MAX_TIME_IN_MS..=MAX_TIME_IN_MS).contains(&date) {
        return None;
    }
    Some(date.trunc() as i64)
}

#[cfg(test)]
mod tests {
    use super::date_parse_ms;

    const JULY_14_2026: i64 = 1_783_987_200_000; // 2026-07-14T00:00:00.000Z

    #[test]
    fn iso_forms_parse_as_before() {
        assert_eq!(date_parse_ms("2026-07-14"), Some(JULY_14_2026));
        assert_eq!(
            date_parse_ms("2026-07-14T09:30:00.000Z"),
            Some(JULY_14_2026 + 34_200_000)
        );
        assert_eq!(
            date_parse_ms("2026-07-14T24:00Z"),
            Some(JULY_14_2026 + 86_400_000)
        );
        assert_eq!(date_parse_ms("2026-07-14T24:01Z"), None);
        assert_eq!(
            date_parse_ms("+010000-01-01T00:00:00Z"),
            Some(253_402_300_800_000)
        );
        assert_eq!(date_parse_ms("-000000-01-01T00:00:00Z"), None);
    }

    #[test]
    fn legacy_forms_parse_as_v8() {
        for s in [
            "July 14, 2026",
            "2026/07/14",
            "2026-7-14",
            "Tue Jul 14 2026",
            "14 jul 2026",
        ] {
            assert_eq!(date_parse_ms(s), Some(JULY_14_2026), "{s}");
        }
        // Full-width digits are word characters: "07-14" of the default year 1 → 2001.
        assert_eq!(date_parse_ms("２０２６-07-14"), date_parse_ms("2001-07-14"));
        assert_eq!(date_parse_ms("July 14"), date_parse_ms("2001-07-14"));
        assert_eq!(date_parse_ms("garbage"), None);
        assert_eq!(date_parse_ms(""), None);
    }
}
