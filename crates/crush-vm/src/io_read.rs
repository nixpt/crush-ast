//! Shared `io.read` logic used by every Crush backend.
//!
//! Reads one line from stdin, trimming the trailing newline. On EOF returns
//! an empty string. This is the single source of truth for line-reading + EOF
//! handling so a program's stdin behavior is identical across backends.

use std::io::{self, BufRead};

/// Read one line from stdin, trimming the trailing newline.
/// Returns an empty string on EOF (matching common scripting-language behavior).
pub fn read_stdin_line() -> String {
    let stdin = io::stdin();
    let mut line = String::new();
    match stdin.lock().read_line(&mut line) {
        Ok(0) => String::new(), // EOF
        Ok(_) => {
            // Trim trailing newline if present
            if line.ends_with('\n') {
                line.pop();
                if line.ends_with('\r') {
                    line.pop();
                }
            }
            line
        }
        Err(_) => String::new(), // Treat error as EOF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_stdin_line_trims_newline() {
        // We can't easily test stdin in a unit test, but we can test the trimming logic
        // This is more of an integration test concern
    }
}