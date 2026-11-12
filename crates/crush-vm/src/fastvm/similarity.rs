//! String similarity calculation algorithms for FastVM.

/// Calculate string similarity using bit-parallel Levenshtein distance (normalized to 0.0 - 1.0)
pub fn calculate_similarity(s1: &str, s2: &str) -> f64 {
    // Use bit-parallel algorithm for performance
    let distance = bit_parallel_levenshtein(s1, s2);

    let len1 = s1.chars().count();
    let len2 = s2.chars().count();

    if len1 == 0 && len2 == 0 {
        return 1.0;
    }

    let max_len = len1.max(len2);
    if max_len == 0 {
        return 1.0;
    }

    // Normalize to 0.0 - 1.0 range where 1.0 is identical
    1.0 - (distance as f64 / max_len as f64)
}

/// Bit-parallel Levenshtein distance calculation
/// Optimized for short to medium length strings (up to 64 characters)
fn bit_parallel_levenshtein(s1: &str, s2: &str) -> usize {
    let chars1: Vec<char> = s1.chars().collect();
    let chars2: Vec<char> = s2.chars().collect();

    let len1 = chars1.len();
    let len2 = chars2.len();

    // Handle empty strings
    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    // For very long strings, fall back to standard algorithm
    if len1 > 64 || len2 > 64 {
        return standard_levenshtein(&chars1, &chars2);
    }

    // Bit-parallel algorithm for strings up to 64 characters
    // Each character gets a bit position in a 64-bit word
    let mut char_masks: [u64; 256] = [0; 256];

    // Build character masks for s1
    for (i, &c) in chars1.iter().enumerate() {
        let byte_val = c as u8;
        char_masks[byte_val as usize] |= 1u64 << i;
    }

    // Initialize bit vectors
    let mut pv = !0u64; // 1s where potential matches exist
    let mut mv = 0u64; // 0s where potential matches exist
    let mut eq: u64;
    let mut xv: u64;
    let mut xh: u64;
    let mut ph: u64;
    let mut mh: u64;

    let mut score = len1 as u64;

    // Process each character in s2
    for &c2 in &chars2 {
        // Get the bit mask for this character
        let byte_val = c2 as u8;
        eq = char_masks[byte_val as usize];

        // xv = mv | eq
        xv = mv | eq;

        // xh = ((pv + (mv & pv)) ^ pv) | mv
        xh = ((pv + (mv & pv)) ^ pv) | mv;

        // ph = mv | ~(xh | pv)
        ph = mv | !(xh | pv);

        // mh = pv & xh
        mh = pv & xh;

        // Update score
        if ph & (1u64 << (len1 - 1)) != 0 {
            score += 1;
        }
        if mh & (1u64 << (len1 - 1)) != 0 {
            score -= 1;
        }

        // Shift operations
        ph <<= 1;
        mh <<= 1;

        // Update pv and mv
        pv = mh | !(xv | ph);
        mv = ph & xv;
    }

    score as usize
}

/// Standard dynamic programming Levenshtein distance for longer strings
fn standard_levenshtein(chars1: &[char], chars2: &[char]) -> usize {
    let len1 = chars1.len();
    let len2 = chars2.len();

    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut prev = vec![0; len2 + 1];
    let mut curr = vec![0; len2 + 1];

    // Initialize first row
    for (j, slot) in prev.iter_mut().enumerate() {
        *slot = j;
    }

    for i in 1..=len1 {
        curr[0] = i;

        for j in 1..=len2 {
            let cost = if chars1[i - 1] == chars2[j - 1] { 0 } else { 1 };

            curr[j] = (prev[j] + 1) // deletion
                .min(curr[j - 1] + 1) // insertion
                .min(prev[j - 1] + cost); // substitution
        }

        prev.copy_from_slice(&curr);
    }

    curr[len2]
}
