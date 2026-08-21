//! Shared `io.read` line-reading semantics used by the CVM1 backends.
//!
//! A read returns one line without its trailing LF/CRLF terminator. EOF is
//! represented by an empty string, matching the behavior of common scripting
//! language input helpers and keeping EOF usable in ordinary Crush control
//! flow.

use std::io::{self, BufRead};

/// Read one line from a buffered input source and remove its line terminator.
///
/// Read errors are returned to the caller. A source that is already at EOF
/// yields an empty string, the documented `io.read` EOF convention.
pub fn read_io_line_from<R: BufRead>(reader: &mut R) -> io::Result<String> {
    let mut line = String::new();
    reader.read_line(&mut line)?;
    while matches!(line.as_bytes().last(), Some(b'\n' | b'\r')) {
        line.pop();
    }
    Ok(line)
}

/// Read one line from the process stdin for the `io.read` capability.
pub fn read_io_line() -> io::Result<String> {
    let stdin = io::stdin();
    let mut reader = stdin.lock();
    read_io_line_from(&mut reader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn strips_lf_and_crlf_terminators() {
        let mut input = Cursor::new(b"first\nsecond\r\nthird");
        assert_eq!(read_io_line_from(&mut input).unwrap(), "first");
        assert_eq!(read_io_line_from(&mut input).unwrap(), "second");
        assert_eq!(read_io_line_from(&mut input).unwrap(), "third");
    }

    #[test]
    fn eof_returns_empty_string() {
        let mut input = Cursor::new(b"");
        assert_eq!(read_io_line_from(&mut input).unwrap(), "");
    }
}
