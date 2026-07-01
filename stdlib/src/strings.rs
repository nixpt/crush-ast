fn starts_with(s: String, prefix: String) -> bool {
    if s.len() >= prefix.len() {
        s[..prefix.len()] == prefix
    } else {
        false
    }
}

fn trim(s: String) -> String {
    let chars: &[u8] = s.as_bytes();
    let mut start = 0;
    let mut end = s.len();
    while start < end && chars[start] <= b' ' {
        start += 1;
    }
    while end > start && chars[end - 1] <= b' ' {
        end -= 1;
    }
    if start >= end {
        String::new()
    } else {
        s[start..end].to_string()
    }
}

fn contains(s: String, substr: String) -> bool {
    if substr.is_empty() {
        return true;
    }
    let s_bytes = s.as_bytes();
    let sub_bytes = substr.as_bytes();
    if sub_bytes.len() > s_bytes.len() {
        return false;
    }
    let limit = s_bytes.len() - sub_bytes.len();
    let mut i = 0;
    while i <= limit {
        let mut matched = true;
        for j in 0..sub_bytes.len() {
            if s_bytes[i + j] != sub_bytes[j] {
                matched = false;
                break;
            }
        }
        if matched {
            return true;
        }
        i += 1;
    }
    false
}

fn to_uppercase(s: String) -> String {
    let mut result = String::new();
    for ch in s.as_bytes() {
        if ch >= &b'a' && ch <= &b'z' {
            result.push((ch - 32) as char);
        } else {
            result.push(*ch as char);
        }
    }
    result
}

fn to_lowercase(s: String) -> String {
    let mut result = String::new();
    for ch in s.as_bytes() {
        if ch >= &b'A' && ch <= &b'Z' {
            result.push((ch + 32) as char);
        } else {
            result.push(*ch as char);
        }
    }
    result
}

fn substring(s: String, start: usize, end: usize) -> String {
    if start >= s.len() || start >= end {
        String::new()
    } else {
        let e = if end > s.len() { s.len() } else { end };
        s[start..e].to_string()
    }
}

fn replace(s: String, from: String, to: String) -> String {
    if from.is_empty() {
        return s;
    }
    let mut result = String::new();
    let s_bytes = s.as_bytes();
    let from_bytes = from.as_bytes();
    let mut i = 0;
    while i < s.len() {
        let mut matched = true;
        if i + from_bytes.len() <= s.len() {
            for j in 0..from_bytes.len() {
                if s_bytes[i + j] != from_bytes[j] {
                    matched = false;
                    break;
                }
            }
        } else {
            matched = false;
        }
        if matched {
            result.push_str(&to);
            i += from_bytes.len();
        } else {
            result.push(s_bytes[i] as char);
            i += 1;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_starts_with() {
        assert!(starts_with("hello world".to_string(), "hello".to_string()));
        assert!(!starts_with("hello world".to_string(), "world".to_string()));
        assert!(starts_with("hi".to_string(), "".to_string()));
    }

    #[test]
    fn test_trim() {
        assert_eq!(trim("  hello  ".to_string()), "hello");
        assert_eq!(trim("\t \n \r".to_string()), "");
        assert_eq!(trim("no spaces".to_string()), "no spaces");
    }

    #[test]
    fn test_contains() {
        assert!(contains("hello world".to_string(), "world".to_string()));
        assert!(!contains("hello world".to_string(), "bye".to_string()));
        assert!(contains("hello".to_string(), "".to_string()));
    }

    #[test]
    fn test_to_uppercase() {
        assert_eq!(to_uppercase("hello World 123!".to_string()), "HELLO WORLD 123!");
    }

    #[test]
    fn test_to_lowercase() {
        assert_eq!(to_lowercase("HELLO world 123!".to_string()), "hello world 123!");
    }

    #[test]
    fn test_substring() {
        assert_eq!(substring("hello world".to_string(), 0, 5), "hello");
        assert_eq!(substring("hello world".to_string(), 6, 11), "world");
        assert_eq!(substring("hello world".to_string(), 6, 20), "world");
        assert_eq!(substring("hello world".to_string(), 20, 30), "");
    }

    #[test]
    fn test_replace() {
        assert_eq!(replace("hello world".to_string(), "world".to_string(), "there".to_string()), "hello there");
        assert_eq!(replace("banana".to_string(), "a".to_string(), "o".to_string()), "bonono");
        assert_eq!(replace("hello".to_string(), "".to_string(), "x".to_string()), "hello");
    }
}
