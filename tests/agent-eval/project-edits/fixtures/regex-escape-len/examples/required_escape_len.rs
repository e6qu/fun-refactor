fn main() {
    for (text, bytes) in [("", 0usize), ("a.b", 4), ("é", 2)] {
        assert_eq!(regex::escape_len(text), bytes);
        assert_eq!(regex_syntax::escape_len(text), bytes);
    }
}
