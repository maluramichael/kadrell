//! Kürzen in der Mitte (`lineBreakMode = .byTruncatingMiddle`, `Text.truncationMode(.middle)`): Anfang und Ende bleiben,
//! in der Mitte steht eine Auslassung. Die Breite liefert der Aufrufer, damit der Kern ohne Schrift testbar bleibt.

const ELLIPSIS: char = '…';

/// `text`, in der Mitte gekürzt, bis `width_of` höchstens `max` meldet. Passt der Text, bleibt er unverändert.
pub fn middle(text: &str, max: f32, width_of: &dyn Fn(&str) -> f32) -> String {
    if width_of(text) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    // Größte Zahl erhaltener Zeichen, die noch passt (die Breite wächst mit der Zahl): Halbierung statt Zählen.
    let (mut lo, mut hi) = (0, chars.len());
    while lo < hi {
        let keep = (lo + hi).div_ceil(2);
        if width_of(&joined(&chars, keep)) <= max {
            lo = keep;
        } else {
            hi = keep - 1;
        }
    }
    joined(&chars, lo)
}

/// `keep` Zeichen: der Anfang bekommt das größere Stück, dazwischen die Auslassung.
fn joined(chars: &[char], keep: usize) -> String {
    let tail = keep / 2;
    let head = keep - tail;
    let mut out: String = chars[..head].iter().collect();
    out.push(ELLIPSIS);
    out.extend(&chars[chars.len() - tail..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(s: &str) -> f32 {
        s.chars().count() as f32 * 10.
    }

    #[test]
    fn fitting_text_stays() {
        assert_eq!(middle("0.5.1", 50., &width), "0.5.1");
    }

    #[test]
    fn long_text_keeps_head_and_tail() {
        let shown = middle("ollama version 0.5.1-rc2", 100., &width);
        assert!(width(&shown) <= 100.);
        assert!(shown.starts_with("ollam") && shown.ends_with("rc2"), "{shown}");
        assert!(shown.contains('…'));
    }

    #[test]
    fn nothing_fits_gives_only_the_ellipsis() {
        assert_eq!(middle("abcdef", 5., &width), "…");
    }
}
