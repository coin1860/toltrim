//! ANSI escape removal. Trimming never runs before this, and a preserved
//! (machine-readable) Invocation runs only this.

/// Remove ANSI escape sequences (CSI, OSC, and lone escapes) from `input`.
///
/// Only escapes are removed; no character is added, reordered, or dropped
/// otherwise, so callers can rely on line structure surviving unchanged.
pub fn strip(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            // CSI: ESC [ ... final byte in 0x40..=0x7E
            Some('[') => {
                chars.next();
                for c2 in chars.by_ref() {
                    if ('@'..='~').contains(&c2) {
                        break;
                    }
                }
            }
            // OSC: ESC ] ... terminated by BEL or ESC \
            Some(']') => {
                chars.next();
                while let Some(c2) = chars.next() {
                    if c2 == '\u{7}' {
                        break;
                    }
                    if c2 == '\u{1b}' {
                        if chars.peek() == Some(&'\\') {
                            chars.next();
                        }
                        break;
                    }
                }
            }
            // Any other escape: drop the escape and the following byte.
            Some(_) => {
                chars.next();
            }
            None => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::strip;

    #[test]
    fn removes_colour_and_keeps_text() {
        assert_eq!(strip("\u{1b}[1;32mBUILD SUCCESS\u{1b}[0m"), "BUILD SUCCESS");
    }

    #[test]
    fn removes_osc_title_sequence() {
        assert_eq!(strip("\u{1b}]0;title\u{7}kept"), "kept");
    }

    #[test]
    fn leaves_plain_text_untouched() {
        assert_eq!(strip("plain text\nsecond line"), "plain text\nsecond line");
    }
}
