pub(crate) fn truncate(value: &str, limit: usize) -> String {
    let mut output = String::with_capacity(value.len().min(limit) + 24);
    append_truncated(&mut output, value, limit);
    output
}

pub(crate) fn append_truncated(output: &mut String, value: &str, limit: usize) {
    if value.len() <= limit {
        output.push_str(value);
        return;
    }

    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    output.push_str(&value[..end]);
    output.push_str("\n... output truncated ...");
}

pub(crate) fn estimate_tokens(value: &str) -> usize {
    let mut ascii_bytes = 0;
    let mut non_ascii_tokens = 0;
    for character in value.chars() {
        if character.is_ascii() {
            ascii_bytes += character.len_utf8();
        } else {
            non_ascii_tokens += 1;
        }
    }
    non_ascii_tokens + ascii_bytes.div_ceil(4)
}
