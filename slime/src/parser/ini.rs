//! A basic zero-copy INI parser
//!
//! It supports single-line comments with the `#` when used at the start of a line.  
//! Invalid lines (keys without values or vice-versa) are ignored.  
//! Whitespace is not trimmed from keys or values.  

/// An INI Parser
///
/// # Example:
///
/// ```rust
/// use slime::parser::ini::*;
/// const INI: &str = r"
///    [foo]
///    a=1
///    b=2
///    # This is a comment and this value won't be recognized; b=3
///    c=3
///    [bar]
///    hello=hallo
///    ";
///
/// for Pair {section, key, value} in Parser::from(INI) {
///     println!("{section}.{key}={value}");
/// }
///
/// ```
#[derive(Debug, Clone)]
pub struct Parser<'a> {
    lines: std::str::Lines<'a>,
    section: &'a str,
}

impl<'a> From<&'a str> for Parser<'a> {
    fn from(s: &'a str) -> Self {
        Self {
            lines: s.lines(),
            section: "",
        }
    }
}

impl<'a> Iterator for Parser<'a> {
    type Item = Pair<'a>;

    fn next(&mut self) -> Option<Pair<'a>> {
        while let Some(line) = self.lines.next() {
            let line = line.trim();
            if line.starts_with("#") {
                continue;
            } else if line.starts_with("[") && line.ends_with("]") {
                // `line` is at least 2 characters long.
                self.section = line.get(1..line.len() - 1).unwrap();
            } else if let Some((key, value)) = line.split_once('=') {
                return Some(Pair {
                    section: self.section,
                    key,
                    value,
                });
            }
        }
        None
    }
}

/// A Key/Value pair from an INI [`Parser`].
#[derive(Debug, Clone, Copy)]
pub struct Pair<'a> {
    /// If a key/value pair was found prior to a section header, or the section header was empty,
    /// then `section` is an empty string `""`.
    pub section: &'a str,
    pub key: &'a str,
    pub value: &'a str,
}
