use std::collections::HashMap;

pub fn git_branch() -> Option<String> {
    std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
            None
        })
}

pub fn parse_cli_args(args: &[&str]) -> (Option<String>, HashMap<String, String>) {
    let mut flags = HashMap::new();
    let mut input = None;
    let mut i = 0;
    while i < args.len() {
        let arg = args[i];
        if arg.starts_with("--") {
            let flag = arg[2..].to_string();
            if i + 1 < args.len() && !args[i + 1].starts_with('-') {
                flags.insert(flag, args[i + 1].to_string());
                i += 1;
            } else {
                flags.insert(flag, "".to_string());
            }
        } else if arg.starts_with('-') && arg.len() > 1 {
            let packed = &arg[1..];
            for c in packed.chars() {
                flags.insert(c.to_string(), "".to_string());
            }
        } else if input.is_none() {
            input = Some(arg.to_string());
        }
        i += 1;
    }
    (input, flags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cli_args_simple() {
        let args = &["file.txt"];
        let (input, flags) = parse_cli_args(args);
        assert_eq!(input, Some("file.txt".to_string()));
        assert!(flags.is_empty());
    }

    #[test]
    fn test_parse_cli_args_flags() {
        let args = &["-l", "-a", "file.txt"];
        let (input, flags) = parse_cli_args(args);
        assert_eq!(input, Some("file.txt".to_string()));
        assert!(flags.contains_key("l"));
        assert!(flags.contains_key("a"));
    }

    #[test]
    fn test_parse_cli_args_long_flag() {
        let args = &["--output", "out.txt", "--verbose"];
        let (input, flags) = parse_cli_args(args);
        assert_eq!(flags.get("output"), Some(&"out.txt".to_string()));
        assert_eq!(flags.get("verbose"), Some(&"".to_string()));
        assert_eq!(input, None);
    }
}
