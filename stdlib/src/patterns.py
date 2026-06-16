def match(pattern, text):
    pi = 0
    ti = 0
    star_pos = 0
    has_star = False
    match_ti = 0
    while ti < len(text):
        if pi < len(pattern):
            if pattern[pi] == text[ti] or pattern[pi] == '?':
                pi = pi + 1
                ti = ti + 1
            elif pattern[pi] == '*':
                star_pos = pi
                has_star = True
                match_ti = ti
                pi = pi + 1
            elif has_star:
                pi = star_pos + 1
                match_ti = match_ti + 1
                ti = match_ti
            else:
                return False
        elif has_star:
            pi = star_pos + 1
            match_ti = match_ti + 1
            ti = match_ti
        else:
            return False
    while pi < len(pattern):
        if pattern[pi] == '*':
            pi = pi + 1
        else:
            break
    if pi == len(pattern):
        return True
    return False

def count_matches(text, substr):
    count = 0
    i = 0
    while i <= len(text) - len(substr):
        found = True
        j = 0
        while j < len(substr):
            if text[i + j] < substr[j] or text[i + j] > substr[j]:
                found = False
            j = j + 1
        if found:
            count = count + 1
        i = i + 1
    return count

def is_digit(c):
    if c >= '0':
        if c <= '9':
            return True
    return False

def is_alpha(c):
    if c >= 'a':
        if c <= 'z':
            return True
    if c >= 'A':
        if c <= 'Z':
            return True
    return False

def count_words(text):
    count = 0
    in_word = False
    i = 0
    while i < len(text):
        if is_alpha(text[i]):
            if in_word == False:
                count = count + 1
                in_word = True
        else:
            in_word = False
        i = i + 1
    return count
