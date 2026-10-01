use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Compile,
    Match,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
    code: i32,
    offset: Option<usize>,
}

impl Error {
    pub(crate) fn compile(err: u32, offset: usize) -> Error {
        Error {
            kind: ErrorKind::Compile,
            code: 100 + err as i32,
            offset: Some(offset),
        }
    }

    pub(crate) fn matching(code: i32) -> Error {
        Error {
            kind: ErrorKind::Match,
            code,
            offset: None,
        }
    }

    #[cfg(test)]
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn code(&self) -> i32 {
        self.code
    }

    #[cfg(test)]
    pub fn offset(&self) -> Option<usize> {
        self.offset
    }

    pub fn message(&self) -> &'static str {
        match self.kind {
            ErrorKind::Compile => {
                let idx = (self.code - 100) as usize;
                COMPILE_MESSAGES
                    .get(idx)
                    .copied()
                    .unwrap_or("internal error")
            }
            ErrorKind::Match => {
                let idx = self.code.unsigned_abs() as usize;
                MATCH_MESSAGES.get(idx).copied().unwrap_or("internal error")
            }
        }
    }
}

impl std::error::Error for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            ErrorKind::Compile => match self.offset {
                Some(off) => write!(
                    f,
                    "PCRE2: error compiling pattern at offset {}: {}",
                    off,
                    self.message()
                ),
                None => write!(f, "PCRE2: error compiling pattern: {}", self.message()),
            },
            ErrorKind::Match => write!(f, "PCRE2: error matching: {}", self.message()),
        }
    }
}

pub const MATCH_LIMIT: i32 = -47;
pub const DEPTH_LIMIT: i32 = -53;
pub const HEAP_LIMIT: i32 = -63;
pub const RECURSE_LOOP: i32 = -52;
pub const JIT_STACK_LIMIT: i32 = -46;
pub const BAD_UTF_OFFSET: i32 = -36;

static COMPILE_MESSAGES: &[&str] = &[
    "no error",
    "\\ at end of pattern",
    "\\c at end of pattern",
    "unrecognized character follows \\",
    "numbers out of order in {} quantifier",
    "number too big in {} quantifier",
    "missing terminating ] for character class",
    "escape sequence is invalid in character class",
    "range out of order in character class",
    "quantifier does not follow a repeatable item",
    "internal error: unexpected repeat",
    "unrecognized character after (? or (?-",
    "POSIX named classes are supported only within a class",
    "POSIX collating elements are not supported",
    "missing closing parenthesis",
    "reference to non-existent subpattern",
    "pattern passed as NULL with non-zero length",
    "unrecognised compile-time option bit(s)",
    "missing ) after (?# comment",
    "parentheses are too deeply nested",
    "regular expression is too large",
    "failed to allocate heap memory",
    "unmatched closing parenthesis",
    "internal error: code overflow",
    "missing closing parenthesis for condition",
    "length of lookbehind assertion is not limited",
    "a relative value of zero is not allowed",
    "conditional subpattern contains more than two branches",
    "atomic assertion expected after (?( or (?(?C)",
    "digit expected after (?+",
    "unknown POSIX class name",
    "internal error in pcre2_study(): should not occur",
    "this version of PCRE2 does not have Unicode support",
    "parentheses are too deeply nested (stack check)",
    "character code point value in \\x{} or \\o{} is too large",
    "lookbehind is too complicated",
    "\\C is not allowed in a lookbehind assertion in UTF-8 mode",
    "PCRE2 does not support \\F, \\L, \\l, \\N{name}, \\U, or \\u",
    "number after (?C is greater than 255",
    "closing parenthesis for (?C expected",
    "invalid escape sequence in (*VERB) name",
    "unrecognized character after (?P",
    "syntax error in subpattern name (missing terminator?)",
    "two named subpatterns have the same name (PCRE2_DUPNAMES not set)",
    "subpattern name must start with a non-digit",
    "this version of PCRE2 does not have support for \\P, \\p, or \\X",
    "malformed \\P or \\p sequence",
    "unknown property after \\P or \\p",
    "subpattern name is too long (maximum 128 code units)",
    "too many named subpatterns (maximum 10000)",
    "invalid range in character class",
    "octal value is greater than \\377 in 8-bit non-UTF-8 mode",
    "internal error: overran compiling workspace",
    "internal error: previously-checked referenced subpattern not found",
    "DEFINE subpattern contains more than one branch",
    "missing opening brace after \\o",
    "internal error: unknown newline setting",
    "\\g is not followed by a braced, angle-bracketed, or quoted name/number or by a plain number",
    "(?R (recursive pattern call) must be followed by a closing parenthesis",
    "obsolete error (should not occur)",
    "(*VERB) not recognized or malformed",
    "subpattern number is too big",
    "subpattern name expected",
    "internal error: parsed pattern overflow",
    "non-octal character in \\o{} (closing brace missing?)",
    "different names for subpatterns of the same number are not allowed",
    "(*MARK) must have an argument",
    "non-hex character in \\x{} (closing brace missing?)",
    "\\c must be followed by a printable ASCII character",
    "\\k is not followed by a braced, angle-bracketed, or quoted name",
    "internal error: unknown meta code in check_lookbehinds()",
    "\\N is not supported in a class",
    "callout string is too long",
    "disallowed Unicode code point (>= 0xd800 && <= 0xdfff)",
    "using UTF is disabled by the application",
    "using UCP is disabled by the application",
    "name is too long in (*MARK), (*PRUNE), (*SKIP), or (*THEN)",
    "character code point value in \\u.... sequence is too large",
    "digits missing after \\x or in \\x{} or \\o{} or \\N{U+}",
    "syntax error or number too big in (?(VERSION condition",
    "internal error: unknown opcode in auto_possessify()",
    "missing terminating delimiter for callout with string argument",
    "unrecognized string delimiter follows (?C",
    "using \\C is disabled by the application",
    "(?| and/or (?J: or (?x: parentheses are too deeply nested",
    "using \\C is disabled in this PCRE2 library",
    "regular expression is too complicated",
    "lookbehind assertion is too long",
    "pattern string is longer than the limit set by the application",
    "internal error: unknown code in parsed pattern",
    "internal error: bad code value in parsed_skip()",
    "PCRE2_EXTRA_ALLOW_SURROGATE_ESCAPES is not allowed in UTF-16 mode",
    "invalid option bits with PCRE2_LITERAL",
    "\\N{U+dddd} is supported only in Unicode (UTF) mode",
    "invalid hyphen in option setting",
    "(*alpha_assertion) not recognized",
    "script runs require Unicode support, which this version of PCRE2 does not have",
    "too many capturing groups (maximum 65535)",
    "octal digit missing after \\0 (PCRE2_EXTRA_NO_BS0 is set)",
    "\\K is not allowed in lookarounds (but see PCRE2_EXTRA_ALLOW_LOOKAROUND_BSK)",
    "branch too long in variable-length lookbehind assertion",
    "compiled pattern would be longer than the limit set by the application",
    "octal value given by \\ddd is greater than \\377 (forbidden by PCRE2_EXTRA_PYTHON_OCTAL)",
    "using callouts is disabled by the application",
    "PCRE2_EXTRA_TURKISH_CASING require Unicode (UTF or UCP) mode",
    "PCRE2_EXTRA_TURKISH_CASING requires UTF in 8-bit mode",
    "PCRE2_EXTRA_TURKISH_CASING and PCRE2_EXTRA_CASELESS_RESTRICT are not compatible",
    "extended character class nesting is too deep",
    "invalid operator in extended character class",
    "unexpected operator in extended character class (no preceding operand)",
    "expected operand after operator in extended character class",
    "square brackets needed to clarify operator precedence in extended character class",
    "missing terminating ] for extended character class (note '[' must be escaped under PCRE2_ALT_EXTENDED_CLASS)",
    "unexpected expression in extended character class (no preceding operator)",
    "empty expression in extended character class",
    "terminating ] with no following closing parenthesis in (?[...]",
    "unexpected character in (?[...]) extended character class",
    "expected capture group number or name",
    "missing opening parenthesis",
    "syntax error in subpattern number (missing terminator?)",
    "erroroffset passed as NULL",
];

static MATCH_MESSAGES: &[&str] = &[
    "no error",
    "no match",
    "partial match",
    "UTF-8 error: 1 byte missing at end",
    "UTF-8 error: 2 bytes missing at end",
    "UTF-8 error: 3 bytes missing at end",
    "UTF-8 error: 4 bytes missing at end",
    "UTF-8 error: 5 bytes missing at end",
    "UTF-8 error: byte 2 top bits not 0x80",
    "UTF-8 error: byte 3 top bits not 0x80",
    "UTF-8 error: byte 4 top bits not 0x80",
    "UTF-8 error: byte 5 top bits not 0x80",
    "UTF-8 error: byte 6 top bits not 0x80",
    "UTF-8 error: 5-byte character is not allowed (RFC 3629)",
    "UTF-8 error: 6-byte character is not allowed (RFC 3629)",
    "UTF-8 error: code points greater than 0x10ffff are not defined",
    "UTF-8 error: code points 0xd800-0xdfff are not defined",
    "UTF-8 error: overlong 2-byte sequence",
    "UTF-8 error: overlong 3-byte sequence",
    "UTF-8 error: overlong 4-byte sequence",
    "UTF-8 error: overlong 5-byte sequence",
    "UTF-8 error: overlong 6-byte sequence",
    "UTF-8 error: isolated byte with 0x80 bit set",
    "UTF-8 error: illegal byte (0xfe or 0xff)",
    "UTF-16 error: missing low surrogate at end",
    "UTF-16 error: invalid low surrogate",
    "UTF-16 error: isolated low surrogate",
    "UTF-32 error: code points 0xd800-0xdfff are not defined",
    "UTF-32 error: code points greater than 0x10ffff are not defined",
    "bad data value",
    "patterns do not all use the same character tables",
    "magic number missing",
    "pattern compiled in wrong mode: 8/16/32-bit error",
    "bad offset value",
    "bad option value",
    "invalid replacement string",
    "bad offset into UTF string",
    "callout error code",
    "invalid data in workspace for DFA restart",
    "too much recursion for DFA matching",
    "backreference condition or recursion test is not supported for DFA matching",
    "function is not supported for DFA matching",
    "pattern contains an item that is not supported for DFA matching",
    "workspace size exceeded in DFA matching",
    "internal error - pattern overwritten?",
    "bad JIT option",
    "JIT stack limit reached",
    "match limit exceeded",
    "no more memory",
    "unknown substring",
    "non-unique substring name",
    "NULL argument passed with non-zero length",
    "nested recursion at the same subject position",
    "matching depth limit exceeded",
    "requested value is not available",
    "requested value is not set",
    "offset limit set without PCRE2_USE_OFFSET_LIMIT",
    "bad escape sequence in replacement string",
    "expected closing curly bracket in replacement string",
    "bad substitution in replacement string",
    "match with end before start or start moved backwards is not supported",
    "too many replacements (more than INT_MAX)",
    "bad serialized data",
    "heap limit exceeded",
    "invalid syntax",
    "internal error: duplicate substitution match",
    "PCRE2_MATCH_INVALID_UTF is not supported for DFA matching",
    "internal error: invalid substring offset",
    "feature is not supported by the JIT compiler",
    "error performing replacement case transformation",
    "replacement too large (longer than PCRE2_SIZE)",
    "substitute pattern differs from prior match call",
    "substitute subject differs from prior match call",
    "substitute start offset differs from prior match call",
    "substitute options differ from prior match call",
    "disallowed use of \\K in lookaround",
    "replacement $' or $_ not supported with partial match",
];
