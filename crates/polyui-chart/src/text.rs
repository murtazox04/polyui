const ELLIPSIS: char = '\u{2026}';

fn char_width_em(character: char) -> f64 {
    match character {
        'i' | 'j' | 'l' | 'I' | '!' | '|' | '.' | ',' | ':' | ';' | '\'' | '`' | 'ʼ' | 'ʻ' => 0.3,
        ' ' | '\u{a0}' | '\u{202f}' | 'f' | 't' | 'r' | '(' | ')' | '[' | ']' | '-' => 0.38,
        'm' | 'w' | 'M' | 'W' | '@' | '%' | 'Ш' | 'Щ' | 'Ж' | 'Ю' | 'Ф' | 'ш' | 'щ' | 'ж' | 'ю'
        | 'ф' | 'Ы' | 'ы' | 'М' | 'м' => 0.9,
        '0'..='9' => 0.58,
        'A'..='Z' | 'А'..='Я' | 'Ё' | 'Ў' | 'Қ' | 'Ғ' | 'Ҳ' => 0.7,
        '\u{1100}'..='\u{115f}'
        | '\u{2e80}'..='\u{a4cf}'
        | '\u{ac00}'..='\u{d7a3}'
        | '\u{f900}'..='\u{faff}'
        | '\u{fe30}'..='\u{fe4f}'
        | '\u{ff00}'..='\u{ff60}'
        | '\u{1f300}'..='\u{1faff}' => 1.0,
        _ => 0.6,
    }
}

pub fn estimate_text_width(text: &str, font_size: f64) -> f64 {
    text.chars().map(char_width_em).sum::<f64>() * font_size
}

pub fn truncate_to_width(text: &str, font_size: f64, max_width: f64) -> String {
    if estimate_text_width(text, font_size) <= max_width {
        return text.to_owned();
    }
    let budget = max_width - char_width_em(ELLIPSIS) * font_size;
    let mut used = 0.0;
    let mut kept = String::new();
    for character in text.chars() {
        let width = char_width_em(character) * font_size;
        if used + width > budget {
            break;
        }
        used += width;
        kept.push(character);
    }
    let trimmed = kept.trim_end();
    if trimmed.is_empty() {
        return ELLIPSIS.to_string();
    }
    format!("{trimmed}{ELLIPSIS}")
}

pub fn label_step(count: usize, slot_width: f64, widest_label: f64, gap: f64) -> usize {
    if count == 0 {
        return 1;
    }
    let needed = widest_label + gap;
    if !slot_width.is_finite() || slot_width <= 0.0 || !needed.is_finite() {
        return count;
    }
    let step = (needed / slot_width).ceil();
    if step.is_finite() && step >= 1.0 {
        (step as usize).clamp(1, count)
    } else {
        1
    }
}

pub fn visible_label_indices(count: usize, step: usize) -> Vec<usize> {
    let step = step.max(1);
    (0..count).step_by(step).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn width_grows_with_text() {
        assert!(estimate_text_width("March", 11.0) > estimate_text_width("Mar", 11.0));
        assert_eq!(estimate_text_width("", 11.0), 0.0);
        assert!(estimate_text_width("WWW", 10.0) > estimate_text_width("iii", 10.0));
    }

    #[test]
    fn truncation_respects_width() {
        let label = truncate_to_width("A very long category name indeed", 11.0, 60.0);
        assert!(label.ends_with(ELLIPSIS));
        assert!(estimate_text_width(&label, 11.0) <= 60.0);
        assert_eq!(truncate_to_width("Jan", 11.0, 60.0), "Jan");
        assert_eq!(truncate_to_width("Jan", 11.0, 0.0), ELLIPSIS.to_string());
    }

    #[test]
    fn thinned_labels_never_overlap() {
        for count in [1usize, 7, 12, 31, 200] {
            for plot_width in [120.0, 300.0, 600.0, 1200.0] {
                let slot = plot_width / count as f64;
                let widest = estimate_text_width("September", 11.0);
                let step = label_step(count, slot, widest, 6.0);
                let shown = visible_label_indices(count, step);
                assert!(!shown.is_empty());
                for pair in shown.windows(2) {
                    let distance = (pair[1] - pair[0]) as f64 * slot;
                    assert!(
                        distance >= widest + 6.0,
                        "overlap at count {count}, width {plot_width}"
                    );
                }
            }
        }
    }

    #[test]
    fn label_step_handles_degenerate_slots() {
        assert_eq!(label_step(0, 10.0, 50.0, 6.0), 1);
        assert_eq!(label_step(10, 0.0, 50.0, 6.0), 10);
        assert_eq!(label_step(10, f64::NAN, 50.0, 6.0), 10);
        assert_eq!(label_step(10, 100.0, 50.0, 6.0), 1);
        assert_eq!(visible_label_indices(5, 0), vec![0, 1, 2, 3, 4]);
    }
}
