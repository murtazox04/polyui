use serde::Serialize;

const MAX_TICKS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ticks {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub values: Vec<f64>,
}

impl Ticks {
    pub fn fraction(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        let fraction = if span.is_finite() && span > 0.0 {
            (value - self.min) / span
        } else {
            let half_span = self.max * 0.5 - self.min * 0.5;
            if half_span > 0.0 {
                (value * 0.5 - self.min * 0.5) / half_span
            } else {
                0.5
            }
        };
        if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    fn fallback(min: f64, max: f64) -> Ticks {
        let values = if min == max {
            vec![min]
        } else {
            vec![min, max]
        };
        Ticks {
            min,
            max,
            step: max - min,
            values,
        }
    }
}

fn nice_step(raw: f64) -> Option<f64> {
    if !raw.is_finite() || raw <= 0.0 {
        return None;
    }
    let magnitude = 10f64.powf(raw.log10().floor());
    if !magnitude.is_finite() || magnitude <= 0.0 {
        return None;
    }
    let normalized = raw / magnitude;
    let multiplier = if normalized >= 50f64.sqrt() {
        10.0
    } else if normalized >= 10f64.sqrt() {
        5.0
    } else if normalized >= 2f64.sqrt() {
        2.0
    } else {
        1.0
    };
    let step = multiplier * magnitude;
    (step.is_finite() && step > 0.0).then_some(step)
}

fn widen_flat(value: f64) -> (f64, f64) {
    if value == 0.0 {
        (0.0, 1.0)
    } else if value > 0.0 {
        (0.0, value)
    } else {
        (value, 0.0)
    }
}

fn snap(value: f64, step: f64) -> f64 {
    let decimals = (-step.log10()).ceil();
    let snapped = if decimals > 0.0 {
        let factor = 10f64.powf(decimals);
        (value * factor).round() / factor
    } else {
        (value / step).round() * step
    };
    let cleaned = if snapped.is_finite() { snapped } else { value };
    if cleaned == 0.0 { 0.0 } else { cleaned }
}

pub fn nice_ticks(low: f64, high: f64, target: usize) -> Ticks {
    let low = if low.is_finite() { low } else { 0.0 };
    let high = if high.is_finite() { high } else { 0.0 };
    let (low, high) = if low <= high {
        (low, high)
    } else {
        (high, low)
    };
    let (low, high) = if low == high {
        widen_flat(low)
    } else {
        (low, high)
    };
    let target = target.clamp(1, MAX_TICKS) as f64;
    let span = high - low;
    let Some(step) = nice_step(span / target) else {
        return Ticks::fallback(low, high);
    };
    let nice_min = (low / step).floor() * step;
    let nice_max = (high / step).ceil() * step;
    let count = (nice_max - nice_min) / step;
    if !nice_min.is_finite() || !nice_max.is_finite() || !count.is_finite() {
        return Ticks::fallback(low, high);
    }
    let count = count.round();
    if count < 1.0 || count > MAX_TICKS as f64 {
        return Ticks::fallback(low, high);
    }
    let count = count as usize;
    let mut values: Vec<f64> = (0..=count)
        .map(|index| snap(nice_min + step * index as f64, step))
        .collect();
    values.dedup();
    if values.len() < 2 {
        return Ticks::fallback(low, high);
    }
    Ticks {
        min: values.first().copied().unwrap_or(nice_min),
        max: values.last().copied().unwrap_or(nice_max),
        step,
        values,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_nice(step: f64) -> bool {
        let magnitude = 10f64.powf(step.log10().floor());
        let normalized = (step / magnitude * 1e9).round() / 1e9;
        [1.0, 2.0, 5.0, 10.0].contains(&normalized)
    }

    #[test]
    fn ticks_cover_the_range_with_nice_steps() {
        let ticks = nice_ticks(0.0, 1234.0, 5);
        assert_eq!(ticks.step, 200.0);
        assert_eq!(ticks.min, 0.0);
        assert_eq!(ticks.max, 1400.0);
        assert_eq!(ticks.values.len(), 8);
        for (low, high) in [
            (0.0, 1.0),
            (-3.0, 17.0),
            (0.001, 0.0093),
            (5e6, 9.1e6),
            (-0.5, -0.1),
        ] {
            let ticks = nice_ticks(low, high, 5);
            assert!(
                is_nice(ticks.step),
                "{low}..{high} gave step {}",
                ticks.step
            );
            assert!(ticks.min <= low && ticks.max >= high);
        }
    }

    #[test]
    fn ticks_have_no_float_noise() {
        let ticks = nice_ticks(0.0, 0.7, 7);
        assert_eq!(ticks.values, vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7]);
    }

    #[test]
    fn negative_ranges_straddle_zero() {
        let ticks = nice_ticks(-45.0, 80.0, 5);
        assert!(ticks.values.contains(&0.0));
        assert_eq!(ticks.step, 20.0);
        assert_eq!(ticks.min, -60.0);
        assert_eq!(ticks.max, 80.0);
    }

    #[test]
    fn flat_and_reversed_ranges() {
        assert_eq!(nice_ticks(0.0, 0.0, 5).max, 1.0);
        let flat = nice_ticks(7.0, 7.0, 5);
        assert_eq!(flat.min, 0.0);
        assert!(flat.max >= 7.0);
        let negative = nice_ticks(-7.0, -7.0, 5);
        assert!(negative.min <= -7.0 && negative.max == 0.0);
        assert_eq!(nice_ticks(10.0, 0.0, 5), nice_ticks(0.0, 10.0, 5));
    }

    #[test]
    fn extreme_ranges_fall_back_safely() {
        let huge = nice_ticks(-f64::MAX, f64::MAX, 5);
        assert!(huge.values.iter().all(|value| value.is_finite()));
        assert!(huge.fraction(0.0) > 0.4 && huge.fraction(0.0) < 0.6);
        let broken = nice_ticks(f64::NAN, f64::INFINITY, 5);
        assert!(broken.values.iter().all(|value| value.is_finite()));
        let tiny = nice_ticks(1e16, 1e16 + 2.0, 5);
        assert!(tiny.values.len() >= 2);
        let subnormal = nice_ticks(0.0, 5e-324, 5);
        assert!(subnormal.values.iter().all(|value| value.is_finite()));
        assert_eq!(nice_ticks(0.0, 10.0, 0).values.len(), 2);
    }

    #[test]
    fn fraction_is_clamped() {
        let ticks = nice_ticks(0.0, 10.0, 5);
        assert_eq!(ticks.fraction(5.0), 0.5);
        assert_eq!(ticks.fraction(-100.0), 0.0);
        assert_eq!(ticks.fraction(f64::NAN), 0.0);
    }
}
