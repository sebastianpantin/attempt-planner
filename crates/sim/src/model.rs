use crate::rules::Weight;

/// Chance of making a lift as a logistic function of load ÷ day max.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct MakeRateCurve {
    /// The load ÷ day max ratio at which the make rate is 50%.
    pub ratio_at_half: f64,
    /// How quickly the make rate falls as the ratio rises past `ratio_at_half`.
    pub steepness: f64,
    /// Chance of missing an attempt whatever the load, for example on depth or a command.
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub technical_miss_chance: f64,
}

impl MakeRateCurve {
    /// Chance of making `load`: no technical miss, and the load is not too heavy.
    pub fn make_probability(&self, load: Weight, day_max_kg: f64) -> f64 {
        self.make_probability_kg(load.kg(), day_max_kg)
    }

    /// Like [`make_probability`](Self::make_probability) for any load in kg, such as a
    /// past attempt in pounds converted to kg.
    pub fn make_probability_kg(&self, load_kg: f64, day_max_kg: f64) -> f64 {
        (1.0 - self.technical_miss_chance) * self.strength_make_probability_kg(load_kg, day_max_kg)
    }

    /// Chance that `load` is not too heavy for a day max of `day_max_kg`.
    pub fn strength_make_probability(&self, load: Weight, day_max_kg: f64) -> f64 {
        self.strength_make_probability_kg(load.kg(), day_max_kg)
    }

    fn strength_make_probability_kg(&self, load_kg: f64, day_max_kg: f64) -> f64 {
        let ratio = load_kg / day_max_kg;
        1.0 / (1.0 + (self.steepness * (ratio - self.ratio_at_half)).exp())
    }

    /// The load below which an attempt with this roll is strong enough. Comparing the load
    /// with it gives the same answer as `roll < strength_make_probability(load, day_max_kg)`,
    /// with the logarithm paid once per roll instead of an exponential per load.
    pub fn make_threshold_kg(&self, day_max_kg: f64, roll: f64) -> f64 {
        day_max_kg * (self.ratio_at_half + (1.0 / roll - 1.0).ln() / self.steepness)
    }

    /// The logarithm of the chance that an attempt at `load_kg` goes as it did, worked out
    /// without rounding to 0 or 1 even far from the day max, so that unlikely results still
    /// rank the day maxes against each other.
    pub fn ln_result_chance(&self, load_kg: f64, day_max_kg: f64, made: bool) -> f64 {
        let past_half = self.steepness * (load_kg / day_max_kg - self.ratio_at_half);
        let ln_strong_enough = -softplus(past_half);
        let ln_too_heavy = -softplus(-past_half);
        let ln_technical = self.technical_miss_chance.ln();
        let ln_no_technical = (-self.technical_miss_chance).ln_1p();
        if made {
            ln_no_technical + ln_strong_enough
        } else {
            ln_add(ln_technical, ln_no_technical + ln_too_heavy)
        }
    }

    pub fn is_valid(&self) -> bool {
        let positive = |x: f64| x.is_finite() && x > 0.0;
        positive(self.ratio_at_half)
            && positive(self.steepness)
            && (0.0..1.0).contains(&self.technical_miss_chance)
    }
}

/// `ln(1 + e^x)` without overflow.
fn softplus(x: f64) -> f64 {
    x.max(0.0) + (-x.abs()).exp().ln_1p()
}

/// `ln(e^a + e^b)` without overflow, minus infinity when both are.
fn ln_add(a: f64, b: f64) -> f64 {
    let high = a.max(b);
    if high == f64::NEG_INFINITY {
        return high;
    }
    high + ((a - high).exp() + (b - high).exp()).ln()
}

/// How much a lifter's max moves on meet day.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct DayForm {
    /// How much the lifter's strength moves from one day to the next, as a standard
    /// deviation over the expected max.
    pub spread: f64,
    /// How far off the expected max itself may be, as a standard deviation over it. Adds to
    /// `spread`, shared between the lifts by the same correlation.
    #[cfg_attr(feature = "serde", serde(default))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub max_uncertainty: f64,
    /// Share of the variation common to all three lifts, from 0 (independent) to 1.
    pub correlation: f64,
}

impl DayForm {
    pub const MAX_SPREAD: f64 = 0.5;
    const MIN_DAY_MAX_FRACTION: f64 = 0.01;

    /// How much the shared and the per-lift draw each move a day max, worked out once for
    /// many days.
    pub fn draw_weights(&self) -> DrawWeights {
        let spread = self.total_spread();
        DrawWeights {
            shared: spread * self.correlation.sqrt(),
            own: spread * (1.0 - self.correlation).sqrt(),
        }
    }

    /// The standard deviation of the day max around the expected max, both parts together.
    pub fn total_spread(&self) -> f64 {
        self.spread.hypot(self.max_uncertainty)
    }

    pub fn is_valid(&self) -> bool {
        self.spread >= 0.0
            && self.max_uncertainty >= 0.0
            && self.total_spread() <= Self::MAX_SPREAD
            && (0.0..=1.0).contains(&self.correlation)
    }
}

/// See [`DayForm::draw_weights`].
#[derive(Debug, Clone, Copy)]
pub struct DrawWeights {
    shared: f64,
    own: f64,
}

impl DrawWeights {
    /// One lift's day max from standard normal draws for the shared and the lift's own part.
    pub fn day_max(&self, expected_kg: f64, shared: f64, own: f64) -> f64 {
        let deviation = self.shared * shared + self.own * own;
        (expected_kg * (1.0 + deviation)).max(expected_kg * DayForm::MIN_DAY_MAX_FRACTION)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lift::PerLift;
    use crate::testing::kg;

    fn day_maxes(
        form: &DayForm,
        expected_kg: &PerLift<f64>,
        shared: f64,
        own: &PerLift<f64>,
    ) -> PerLift<f64> {
        let draws = form.draw_weights();
        expected_kg.map(|lift, &expected| draws.day_max(expected, shared, *own.get(lift)))
    }

    const CURVE: MakeRateCurve = MakeRateCurve {
        ratio_at_half: 1.0,
        steepness: 20.0,
        technical_miss_chance: 0.0,
    };

    #[test]
    fn half_the_attempts_at_the_midpoint_are_made() {
        assert_eq!(CURVE.make_probability(kg(200.0), 200.0), 0.5);
    }

    #[test]
    fn matches_the_logistic_formula() {
        let expected = 1.0 / (1.0 + (20.0_f64 * (0.95 - 1.0)).exp());
        assert!((CURVE.make_probability(kg(190.0), 200.0) - expected).abs() < 1e-12);
    }

    proptest::proptest! {
        #[test]
        fn make_threshold_agrees_with_the_make_probability(
            load in 20_000u32..400_000,
            day_max_kg in 20.0..400.0f64,
            roll in 0.0..1.0f64,
        ) {
            let load = Weight::from_grams(load / 2_500 * 2_500);
            let probability = CURVE.strength_make_probability(load, day_max_kg);
            proptest::prop_assume!((roll - probability).abs() > 1e-9);
            proptest::prop_assert_eq!(
                roll < probability,
                load.kg() < CURVE.make_threshold_kg(day_max_kg, roll)
            );
        }
    }

    #[test]
    fn the_log_chance_of_a_result_matches_the_make_probability() {
        let curve = MakeRateCurve {
            technical_miss_chance: 0.05,
            ..CURVE
        };
        for load_kg in [150.0, 195.0, 200.0, 230.0] {
            let made = curve.make_probability_kg(load_kg, 200.0);
            let chance = |made| curve.ln_result_chance(load_kg, 200.0, made).exp();
            assert!((chance(true) - made).abs() < 1e-12, "{load_kg}");
            assert!((chance(false) - (1.0 - made)).abs() < 1e-12, "{load_kg}");
        }
    }

    #[test]
    fn results_far_from_the_day_max_keep_a_finite_log_chance() {
        let sharp = MakeRateCurve {
            steepness: 2_000.0,
            ..CURVE
        };
        let missed_light = sharp.ln_result_chance(100.0, 200.0, false);
        let made_heavy = sharp.ln_result_chance(300.0, 200.0, true);
        assert!(missed_light.is_finite() && made_heavy.is_finite());
        assert!(missed_light < sharp.ln_result_chance(110.0, 200.0, false));
    }

    #[test]
    fn technical_misses_scale_the_make_probability() {
        let curve = MakeRateCurve {
            technical_miss_chance: 0.1,
            ..CURVE
        };
        let strength = CURVE.make_probability(kg(190.0), 200.0);
        assert!((curve.make_probability(kg(190.0), 200.0) - 0.9 * strength).abs() < 1e-12);
    }

    #[test]
    fn technical_miss_chance_must_leave_room_to_make_an_attempt() {
        let with = |technical_miss_chance| MakeRateCurve {
            technical_miss_chance,
            ..CURVE
        };
        assert!(with(0.0).is_valid() && with(0.5).is_valid());
        for chance in [-0.1, 1.0, f64::NAN] {
            assert!(!with(chance).is_valid(), "{chance}");
        }
    }

    #[test]
    fn a_zero_roll_makes_any_load() {
        assert_eq!(CURVE.make_threshold_kg(200.0, 0.0), f64::INFINITY);
    }

    #[test]
    fn heavier_loads_are_harder() {
        let light = CURVE.make_probability(kg(170.0), 200.0);
        let heavy = CURVE.make_probability(kg(210.0), 200.0);
        assert!(light > 0.9 && heavy < 0.3);
    }

    #[test]
    fn curve_needs_positive_finite_parameters() {
        assert!(CURVE.is_valid());
        for (ratio_at_half, steepness) in [
            (0.0, 20.0),
            (1.0, 0.0),
            (f64::NAN, 20.0),
            (1.0, f64::INFINITY),
        ] {
            assert!(
                !MakeRateCurve {
                    ratio_at_half,
                    steepness,
                    technical_miss_chance: 0.0,
                }
                .is_valid()
            );
        }
    }

    #[test]
    fn no_spread_means_the_expected_max() {
        let form = DayForm {
            spread: 0.0,
            max_uncertainty: 0.0,
            correlation: 0.5,
        };
        let expected = PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        };
        let own = PerLift::from_fn(|_| 1.7);
        assert_eq!(day_maxes(&form, &expected, -2.0, &own), expected);
    }

    #[test]
    fn full_correlation_moves_every_lift_by_the_shared_draw() {
        let form = DayForm {
            spread: 0.05,
            max_uncertainty: 0.0,
            correlation: 1.0,
        };
        let expected = PerLift::from_fn(|_| 200.0);
        let own = PerLift {
            squat: 3.0,
            bench: -3.0,
            deadlift: 0.0,
        };
        let maxes = day_maxes(&form, &expected, -1.0, &own);
        for (_, max) in maxes.iter() {
            assert!((max - 190.0).abs() < 1e-9);
        }
    }

    #[test]
    fn no_correlation_uses_only_each_lifts_own_draw() {
        let form = DayForm {
            spread: 0.05,
            max_uncertainty: 0.0,
            correlation: 0.0,
        };
        let expected = PerLift::from_fn(|_| 200.0);
        let own = PerLift {
            squat: 1.0,
            bench: -1.0,
            deadlift: 0.0,
        };
        let maxes = day_maxes(&form, &expected, 3.0, &own);
        assert!((maxes.squat - 210.0).abs() < 1e-9);
        assert!((maxes.bench - 190.0).abs() < 1e-9);
        assert!((maxes.deadlift - 200.0).abs() < 1e-9);
    }

    #[test]
    fn max_uncertainty_adds_to_the_day_form_spread() {
        let form = DayForm {
            spread: 0.03,
            max_uncertainty: 0.04,
            correlation: 1.0,
        };
        assert!((form.total_spread() - 0.05).abs() < 1e-12);
        let maxes = day_maxes(
            &form,
            &PerLift::from_fn(|_| 200.0),
            1.0,
            &PerLift::from_fn(|_| 0.0),
        );
        assert!((maxes.squat - 210.0).abs() < 1e-9);
    }

    #[test]
    fn day_max_never_drops_to_zero() {
        let form = DayForm {
            spread: 0.5,
            max_uncertainty: 0.0,
            correlation: 1.0,
        };
        let maxes = day_maxes(
            &form,
            &PerLift::from_fn(|_| 200.0),
            -10.0,
            &PerLift::from_fn(|_| 0.0),
        );
        assert!(maxes.squat > 0.0);
    }

    #[test]
    fn day_form_ranges() {
        assert!(
            DayForm {
                spread: 0.03,
                max_uncertainty: 0.0,
                correlation: 0.5
            }
            .is_valid()
        );
        for (spread, max_uncertainty, correlation) in [
            (-0.01, 0.0, 0.5),
            (0.6, 0.0, 0.5),
            (0.03, -0.01, 0.5),
            (0.4, 0.4, 0.5),
            (0.03, f64::NAN, 0.5),
            (0.03, 0.0, 1.1),
            (0.03, 0.0, -0.1),
            (f64::NAN, 0.0, 0.5),
        ] {
            assert!(
                !DayForm {
                    spread,
                    max_uncertainty,
                    correlation
                }
                .is_valid()
            );
        }
    }
}
