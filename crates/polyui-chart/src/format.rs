use crate::spec::Locale;

const NBSP: &str = "\u{a0}";
const SCIENTIFIC_FROM: f64 = 1e15;
const MAX_DECIMALS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Magnitude {
    Thousand,
    Million,
    Billion,
    Trillion,
}

const MAGNITUDES: [(f64, Magnitude); 4] = [
    (1e12, Magnitude::Trillion),
    (1e9, Magnitude::Billion),
    (1e6, Magnitude::Million),
    (1e3, Magnitude::Thousand),
];

fn group_separator(locale: Locale) -> &'static str {
    match locale {
        Locale::En => ",",
        Locale::UzLatn | Locale::UzCyrl | Locale::Ru => NBSP,
    }
}

fn decimal_separator(locale: Locale) -> &'static str {
    match locale {
        Locale::En => ".",
        Locale::UzLatn | Locale::UzCyrl | Locale::Ru => ",",
    }
}

fn short_suffix(magnitude: Magnitude, locale: Locale) -> String {
    let word = match (locale, magnitude) {
        (Locale::En, Magnitude::Thousand) => return "k".to_owned(),
        (Locale::En, Magnitude::Million) => return "M".to_owned(),
        (Locale::En, Magnitude::Billion) => return "B".to_owned(),
        (Locale::En, Magnitude::Trillion) => return "T".to_owned(),
        (Locale::UzLatn, Magnitude::Thousand) => "ming",
        (Locale::UzLatn, Magnitude::Million) => "mln",
        (Locale::UzLatn, Magnitude::Billion) => "mlrd",
        (Locale::UzLatn, Magnitude::Trillion) => "trln",
        (Locale::UzCyrl, Magnitude::Thousand) => "минг",
        (Locale::UzCyrl, Magnitude::Million) => "млн",
        (Locale::UzCyrl, Magnitude::Billion) => "млрд",
        (Locale::UzCyrl, Magnitude::Trillion) => "трлн",
        (Locale::Ru, Magnitude::Thousand) => "тыс.",
        (Locale::Ru, Magnitude::Million) => "млн",
        (Locale::Ru, Magnitude::Billion) => "млрд",
        (Locale::Ru, Magnitude::Trillion) => "трлн",
    };
    format!("{NBSP}{word}")
}

fn spoken_word(magnitude: Magnitude, mantissa: f64, locale: Locale) -> &'static str {
    match locale {
        Locale::En => match magnitude {
            Magnitude::Thousand => "thousand",
            Magnitude::Million => "million",
            Magnitude::Billion => "billion",
            Magnitude::Trillion => "trillion",
        },
        Locale::UzLatn => match magnitude {
            Magnitude::Thousand => "ming",
            Magnitude::Million => "million",
            Magnitude::Billion => "milliard",
            Magnitude::Trillion => "trillion",
        },
        Locale::UzCyrl => match magnitude {
            Magnitude::Thousand => "минг",
            Magnitude::Million => "миллион",
            Magnitude::Billion => "миллиард",
            Magnitude::Trillion => "триллион",
        },
        Locale::Ru => {
            let forms = match magnitude {
                Magnitude::Thousand => ["тысяча", "тысячи", "тысяч"],
                Magnitude::Million => ["миллион", "миллиона", "миллионов"],
                Magnitude::Billion => ["миллиард", "миллиарда", "миллиардов"],
                Magnitude::Trillion => ["триллион", "триллиона", "триллионов"],
            };
            let [one, few, many] = forms;
            match russian_plural(mantissa) {
                RussianPlural::One => one,
                RussianPlural::Few => few,
                RussianPlural::Many => many,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RussianPlural {
    One,
    Few,
    Many,
}

fn russian_plural(value: f64) -> RussianPlural {
    let magnitude = value.abs();
    if magnitude.fract() != 0.0 {
        return RussianPlural::Few;
    }
    let whole = (magnitude % 100.0) as u32;
    let last = whole % 10;
    if last == 1 && whole != 11 {
        RussianPlural::One
    } else if (2..=4).contains(&last) && !(12..=14).contains(&whole) {
        RussianPlural::Few
    } else {
        RussianPlural::Many
    }
}

fn round_to(value: f64, decimals: usize) -> f64 {
    let factor = 10f64.powi(decimals.min(MAX_DECIMALS) as i32);
    let rounded = (value * factor).round() / factor;
    if rounded.is_finite() { rounded } else { value }
}

fn group_digits(digits: &str, separator: &str) -> String {
    let count = digits.chars().count();
    let mut grouped = String::with_capacity(digits.len() + count / 3 * separator.len());
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (count - index).is_multiple_of(3) {
            grouped.push_str(separator);
        }
        grouped.push(digit);
    }
    grouped
}

fn format_plain(value: f64, decimals: usize, locale: Locale) -> String {
    if !value.is_finite() {
        return "?".to_owned();
    }
    if value.abs() >= SCIENTIFIC_FROM {
        return format_scientific(value, locale);
    }
    let decimals = decimals.min(MAX_DECIMALS);
    let text = format!("{:.*}", decimals, value.abs());
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let fraction = fraction.trim_end_matches('0');
    let is_zero = whole.chars().all(|digit| digit == '0') && fraction.is_empty();
    let mut formatted = String::new();
    if value < 0.0 && !is_zero {
        formatted.push('-');
    }
    formatted.push_str(&group_digits(whole, group_separator(locale)));
    if !fraction.is_empty() {
        formatted.push_str(decimal_separator(locale));
        formatted.push_str(fraction);
    }
    formatted
}

fn format_scientific(value: f64, locale: Locale) -> String {
    if !value.is_finite() {
        return "?".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let mut exponent = value.abs().log10().floor() as i32;
    let mut mantissa = round_to(value / 10f64.powi(exponent), 1);
    if mantissa.abs() >= 10.0 {
        exponent += 1;
        mantissa = round_to(value / 10f64.powi(exponent), 1);
    }
    format!("{}E{exponent}", format_plain(mantissa, 1, locale))
}

fn pick_magnitude(value: f64) -> Option<(f64, Magnitude)> {
    let magnitude = value.abs();
    MAGNITUDES
        .iter()
        .copied()
        .find(|(divisor, _)| magnitude >= *divisor)
}

fn next_magnitude(current: Magnitude) -> Option<(f64, Magnitude)> {
    match current {
        Magnitude::Thousand => Some((1e6, Magnitude::Million)),
        Magnitude::Million => Some((1e9, Magnitude::Billion)),
        Magnitude::Billion => Some((1e12, Magnitude::Trillion)),
        Magnitude::Trillion => None,
    }
}

fn compact_parts(value: f64) -> Option<(f64, Magnitude)> {
    let (divisor, magnitude) = pick_magnitude(value)?;
    let mantissa = round_to(value / divisor, 1);
    if mantissa.abs() >= 1000.0
        && let Some((next_divisor, next)) = next_magnitude(magnitude)
    {
        return Some((round_to(value / next_divisor, 1), next));
    }
    Some((mantissa, magnitude))
}

pub fn format_number(value: f64, locale: Locale) -> String {
    format_plain(round_to(value, 2), 2, locale)
}

pub fn format_compact(value: f64, locale: Locale) -> String {
    if !value.is_finite() {
        return "?".to_owned();
    }
    if value.abs() >= SCIENTIFIC_FROM {
        return format_scientific(value, locale);
    }
    match compact_parts(value) {
        Some((mantissa, magnitude)) => format!(
            "{}{}",
            format_plain(mantissa, 1, locale),
            short_suffix(magnitude, locale)
        ),
        None => format_number(value, locale),
    }
}

pub fn format_spoken(value: f64, locale: Locale) -> String {
    if !value.is_finite() {
        return "?".to_owned();
    }
    if value.abs() >= SCIENTIFIC_FROM {
        return format_scientific(value, locale);
    }
    match compact_parts(value) {
        Some((mantissa, magnitude)) => format!(
            "{} {}",
            format_plain(mantissa, 1, locale),
            spoken_word(magnitude, mantissa, locale)
        ),
        None => format_number(value, locale),
    }
}

pub fn format_percent(fraction: f64, locale: Locale) -> String {
    let percent = fraction * 100.0;
    let decimals = if percent.abs() < 10.0 { 1 } else { 0 };
    let number = format_plain(round_to(percent, decimals), decimals, locale);
    match locale {
        Locale::Ru => format!("{number}{NBSP}%"),
        Locale::En | Locale::UzLatn | Locale::UzCyrl => format!("{number}%"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TickFormat {
    locale: Locale,
    divisor: f64,
    magnitude: Option<Magnitude>,
    decimals: usize,
    scientific: bool,
}

impl TickFormat {
    pub fn new(ticks: &[f64], step: f64, locale: Locale) -> Self {
        let largest = ticks
            .iter()
            .copied()
            .filter(|tick| tick.is_finite())
            .map(f64::abs)
            .fold(0.0, f64::max);
        let (divisor, magnitude) = if largest < 1e4 {
            (1.0, None)
        } else {
            match pick_magnitude(largest) {
                Some((divisor, magnitude)) => (divisor, Some(magnitude)),
                None => (1.0, None),
            }
        };
        let scaled_step = step / divisor;
        let decimals = if scaled_step.is_finite() && scaled_step > 0.0 {
            (-scaled_step.log10())
                .ceil()
                .clamp(0.0, MAX_DECIMALS as f64) as usize
        } else {
            2
        };
        TickFormat {
            locale,
            divisor,
            magnitude,
            decimals,
            scientific: largest >= SCIENTIFIC_FROM,
        }
    }

    pub fn format(&self, value: f64) -> String {
        if self.scientific {
            return format_scientific(value, self.locale);
        }
        let scaled = round_to(value / self.divisor, self.decimals);
        let number = format_plain(scaled, self.decimals, self.locale);
        match self.magnitude {
            Some(magnitude) if scaled != 0.0 => {
                format!("{number}{}", short_suffix(magnitude, self.locale))
            }
            _ => number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_numbers_group_per_locale() {
        assert_eq!(format_number(1200.0, Locale::En), "1,200");
        assert_eq!(format_number(1200.0, Locale::UzLatn), "1\u{a0}200");
        assert_eq!(format_number(1200.0, Locale::UzCyrl), "1\u{a0}200");
        assert_eq!(format_number(1200.0, Locale::Ru), "1\u{a0}200");
        assert_eq!(format_number(29322.19, Locale::UzLatn), "29\u{a0}322,19");
        assert_eq!(format_number(1234567.5, Locale::En), "1,234,567.5");
        assert_eq!(format_number(-0.004, Locale::En), "0");
        assert_eq!(format_number(-42.0, Locale::Ru), "-42");
        assert_eq!(format_number(999.0, Locale::En), "999");
    }

    #[test]
    fn compact_numbers_per_locale() {
        assert_eq!(format_compact(1200.0, Locale::En), "1.2k");
        assert_eq!(format_compact(1200.0, Locale::UzLatn), "1,2\u{a0}ming");
        assert_eq!(format_compact(1200.0, Locale::UzCyrl), "1,2\u{a0}минг");
        assert_eq!(format_compact(1200.0, Locale::Ru), "1,2\u{a0}тыс.");
        assert_eq!(format_compact(3_400_000.0, Locale::En), "3.4M");
        assert_eq!(format_compact(3_400_000.0, Locale::UzLatn), "3,4\u{a0}mln");
        assert_eq!(format_compact(3_400_000.0, Locale::UzCyrl), "3,4\u{a0}млн");
        assert_eq!(format_compact(3_400_000.0, Locale::Ru), "3,4\u{a0}млн");
        assert_eq!(format_compact(5e9, Locale::En), "5B");
        assert_eq!(format_compact(5e9, Locale::UzLatn), "5\u{a0}mlrd");
        assert_eq!(format_compact(5e9, Locale::UzCyrl), "5\u{a0}млрд");
        assert_eq!(format_compact(5e9, Locale::Ru), "5\u{a0}млрд");
        assert_eq!(format_compact(-2500.0, Locale::En), "-2.5k");
        assert_eq!(format_compact(42.5, Locale::En), "42.5");
    }

    #[test]
    fn compact_rolls_over_to_next_magnitude() {
        assert_eq!(format_compact(999_960.0, Locale::En), "1M");
        assert_eq!(format_compact(999.999, Locale::En), "1,000");
    }

    #[test]
    fn huge_and_broken_values_never_panic() {
        assert_eq!(format_compact(1e300, Locale::En), "1E300");
        assert_eq!(format_compact(-2.5e20, Locale::Ru), "-2,5E20");
        assert_eq!(format_number(f64::MAX, Locale::En), "1.8E308");
        assert_eq!(format_number(f64::NAN, Locale::En), "?");
        assert_eq!(format_compact(f64::INFINITY, Locale::En), "?");
        assert_eq!(format_spoken(f64::NEG_INFINITY, Locale::En), "?");
        assert_eq!(format_number(5e-324, Locale::En), "0");
    }

    #[test]
    fn spoken_numbers_use_words() {
        assert_eq!(format_spoken(1200.0, Locale::En), "1.2 thousand");
        assert_eq!(format_spoken(1200.0, Locale::UzLatn), "1,2 ming");
        assert_eq!(format_spoken(2e9, Locale::UzCyrl), "2 миллиард");
        assert_eq!(format_spoken(1200.0, Locale::Ru), "1,2 тысячи");
        assert_eq!(format_spoken(1000.0, Locale::Ru), "1 тысяча");
        assert_eq!(format_spoken(3000.0, Locale::Ru), "3 тысячи");
        assert_eq!(format_spoken(5_000_000.0, Locale::Ru), "5 миллионов");
        assert_eq!(format_spoken(21_000.0, Locale::Ru), "21 тысяча");
        assert_eq!(format_spoken(11_000.0, Locale::Ru), "11 тысяч");
        assert_eq!(format_spoken(300.0, Locale::En), "300");
    }

    #[test]
    fn percent_per_locale() {
        assert_eq!(format_percent(0.4, Locale::En), "40%");
        assert_eq!(format_percent(0.025, Locale::UzLatn), "2,5%");
        assert_eq!(format_percent(0.4, Locale::UzCyrl), "40%");
        assert_eq!(format_percent(0.4, Locale::Ru), "40\u{a0}%");
    }

    #[test]
    fn tick_format_shares_one_scale() {
        let small = TickFormat::new(&[0.0, 0.5, 1.0], 0.5, Locale::En);
        assert_eq!(small.format(0.5), "0.5");
        assert_eq!(small.format(1.0), "1");
        let thousands = TickFormat::new(&[0.0, 2000.0, 4000.0], 2000.0, Locale::En);
        assert_eq!(thousands.format(2000.0), "2,000");
        let large = TickFormat::new(&[0.0, 50_000.0, 100_000.0], 50_000.0, Locale::UzLatn);
        assert_eq!(large.format(0.0), "0");
        assert_eq!(large.format(50_000.0), "50\u{a0}ming");
        let fine = TickFormat::new(&[1_000_000.0, 1_050_000.0], 50_000.0, Locale::En);
        assert_eq!(fine.format(1_050_000.0), "1.05M");
        let ru = TickFormat::new(&[0.0, 1e6, 2e6], 1e6, Locale::Ru);
        assert_eq!(ru.format(2e6), "2\u{a0}млн");
    }
}
