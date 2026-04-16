pub const DUMMY_TEXT: &str = "Hej, det här är ett test med å, ä och ö.";

pub fn dummy_transcribe() -> String {
    DUMMY_TEXT.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_text_contains_swedish_chars() {
        let t = dummy_transcribe();
        assert!(t.contains('å'));
        assert!(t.contains('ä'));
        assert!(t.contains('ö'));
    }
}
