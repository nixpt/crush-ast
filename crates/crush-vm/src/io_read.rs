//! Shared `io.read` line-reading semantics used by the CVM1 backends.
//!
//! A read returns one line without its trailing LF/CRLF terminator. EOF is
//! represented by an empty string, matching the behavior of common scripting
//! language input helpers and keeping EOF usable in ordinary Crush control
//! flow.
//!
//! [`InputSource`] lets a host choose where those lines come from: process
//! stdin (the default, and the only source a native CLI needs), text supplied
//! up front, or lines fed one at a time by an interactive host such as a
//! browser page that has no stdin at all.

use std::io::{self, BufRead, Cursor};

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

/// Where a VM's `io.read` lines come from.
///
/// Every variant reads through [`read_io_line_from`], so terminator stripping
/// and the empty-string EOF convention are identical whichever source a host
/// picks.
#[derive(Debug, Clone, Default)]
pub enum InputSource {
    /// Process stdin (default). Native behaviour; unchanged by this type.
    #[default]
    Stdin,
    /// Text given up front. Each `io.read` takes the next line; once it runs
    /// out every read returns `""` (EOF).
    Supplied(Cursor<Vec<u8>>),
    /// Lines fed by the host while the program runs. When nothing is pending
    /// and the host hasn't closed the input, the VM pauses instead of reading
    /// (see [`InputSource::would_block`]); after [`InputSource::close`],
    /// reads with nothing pending return `""` (EOF).
    Interactive {
        pending: Cursor<Vec<u8>>,
        closed: bool,
    },
}

impl InputSource {
    /// A source that serves `text` line by line, then EOF.
    pub fn supplied(text: impl Into<String>) -> Self {
        Self::Supplied(Cursor::new(text.into().into_bytes()))
    }

    /// An interactive source with nothing pending yet.
    pub fn interactive() -> Self {
        Self::Interactive {
            pending: Cursor::new(Vec::new()),
            closed: false,
        }
    }

    /// True when an `io.read` would have to wait for the host: interactive,
    /// nothing pending, input not closed. A VM checks this *before* executing
    /// the read so it can pause with its state untouched.
    pub fn would_block(&self) -> bool {
        match self {
            Self::Interactive { pending, closed } => !*closed && is_drained(pending),
            _ => false,
        }
    }

    /// Queue one line for an interactive source. A line containing `\n`
    /// becomes several lines. Errors if the source isn't interactive or was
    /// closed.
    pub fn provide(&mut self, line: &str) -> Result<(), &'static str> {
        match self {
            Self::Interactive {
                pending,
                closed: false,
            } => {
                if is_drained(pending) {
                    *pending = Cursor::new(Vec::new());
                }
                let buf = pending.get_mut();
                buf.extend_from_slice(line.as_bytes());
                buf.push(b'\n');
                Ok(())
            }
            Self::Interactive { closed: true, .. } => Err("input is closed"),
            _ => Err("input source is not interactive"),
        }
    }

    /// Signal EOF to an interactive source: once pending lines are consumed,
    /// reads return `""` instead of pausing.
    pub fn close(&mut self) {
        if let Self::Interactive { closed, .. } = self {
            *closed = true;
        }
    }

    /// Read the next line for `io.read`.
    pub fn read_line(&mut self) -> io::Result<String> {
        match self {
            Self::Stdin => read_io_line(),
            Self::Supplied(input) | Self::Interactive { pending: input, .. } => {
                read_io_line_from(input)
            }
        }
    }
}

fn is_drained(cursor: &Cursor<Vec<u8>>) -> bool {
    cursor.position() as usize >= cursor.get_ref().len()
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

    #[test]
    fn supplied_serves_lines_then_eof() {
        let mut source = InputSource::supplied("10\r\nh\ns");
        assert!(!source.would_block());
        assert_eq!(source.read_line().unwrap(), "10");
        assert_eq!(source.read_line().unwrap(), "h");
        assert_eq!(source.read_line().unwrap(), "s");
        assert_eq!(source.read_line().unwrap(), "");
        assert_eq!(source.read_line().unwrap(), "");
        assert!(source.provide("x").is_err());
    }

    #[test]
    fn interactive_blocks_until_provided_and_eofs_after_close() {
        let mut source = InputSource::interactive();
        assert!(source.would_block());
        source.provide("first").unwrap();
        source.provide("a\nb").unwrap();
        assert!(!source.would_block());
        assert_eq!(source.read_line().unwrap(), "first");
        assert_eq!(source.read_line().unwrap(), "a");
        assert_eq!(source.read_line().unwrap(), "b");
        assert!(source.would_block());
        source.provide("").unwrap();
        assert_eq!(source.read_line().unwrap(), "");
        assert!(source.would_block());
        source.close();
        assert!(!source.would_block());
        assert_eq!(source.read_line().unwrap(), "");
        assert!(source.provide("late").is_err());
    }
}
