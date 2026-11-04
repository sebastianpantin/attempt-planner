use crate::lift::PerLift;
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
}

impl MakeRateCurve {
    pub fn make_probability(&self, load: Weight, day_max_kg: f64) -> f64 {
        let ratio = load.kg() / day_max_kg;
        1.0 / (1.0 + (self.steepness * (ratio - self.ratio_at_half)).exp())
    }

    /// The load below which an attempt with this roll is made. Comparing the load with it
    /// gives the same answer as `roll < make_probability(load, day_max_kg)`, with the
    /// logarithm paid once per roll instead of an exponential per load.
    pub fn make_threshold_kg(&self, day_max_kg: f64, roll: f64) -> f64 {
        day_max_kg * (self.ratio_at_half + (1.0 / roll - 1.0).ln() / self.steepness)
    }

    pub fn is_valid(&self) -> bool {
        let positive = |x: f64| x.is_finite() && x > 0.0;
        positive(self.ratio_at_half) && positive(self.steepness)
    }
}

/// How much a lifter's max moves on meet day.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct DayForm {
    /// Standard deviation of the day max, as a fraction of the expected max.
    pub spread: f64,
    /// Share of that variation common to all three lifts, from 0 (independent) to 1.
    pub correlation: f64,
}

impl DayForm {
    pub const MAX_SPREAD: f64 = 0.5;
    const MIN_DAY_MAX_FRACTION: f64 = 0.01;

    /// Day max for each lift, from standard normal draws for the shared and per-lift parts.
    pub fn day_maxes(
        &self,
        expected_kg: &PerLift<f64>,
        shared: f64,
        own: &PerLift<f64>,
    ) -> PerLift<f64> {
        let shared_weight = self.correlation.sqrt();
        let own_weight = (1.0 - self.correlation).sqrt();
        expected_kg.map(|lift, &expected| {
            let deviation = self.spread * (shared_weight * shared + own_weight * own.get(lift));
            (expected * (1.0 + deviation)).max(expected * Self::MIN_DAY_MAX_FRACTION)
        })
    }

    pub fn is_valid(&self) -> bool {
        (0.0..=Self::MAX_SPREAD).contains(&self.spread) && (0.0..=1.0).contains(&self.correlation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::kg;

    const CURVE: MakeRateCurve = MakeRateCurve {
        ratio_at_half: 1.0,
        steepness: 20.0,
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
            let probability = CURVE.make_probability(load, day_max_kg);
            proptest::prop_assume!((roll - probability).abs() > 1e-9);
            proptest::prop_assert_eq!(
                roll < probability,
                load.kg() < CURVE.make_threshold_kg(day_max_kg, roll)
            );
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
                    steepness
                }
                .is_valid()
            );
        }
    }

    #[test]
    fn no_spread_means_the_expected_max() {
        let form = DayForm {
            spread: 0.0,
            correlation: 0.5,
        };
        let expected = PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        };
        let own = PerLift::from_fn(|_| 1.7);
        assert_eq!(form.day_maxes(&expected, -2.0, &own), expected);
    }

    #[test]
    fn full_correlation_moves_every_lift_by_the_shared_draw() {
        let form = DayForm {
            spread: 0.05,
            correlation: 1.0,
        };
        let expected = PerLift::from_fn(|_| 200.0);
        let own = PerLift {
            squat: 3.0,
            bench: -3.0,
            deadlift: 0.0,
        };
        let maxes = form.day_maxes(&expected, -1.0, &own);
        for (_, max) in maxes.iter() {
            assert!((max - 190.0).abs() < 1e-9);
        }
    }

    #[test]
    fn no_correlation_uses_only_each_lifts_own_draw() {
        let form = DayForm {
            spread: 0.05,
            correlation: 0.0,
        };
        let expected = PerLift::from_fn(|_| 200.0);
        let own = PerLift {
            squat: 1.0,
            bench: -1.0,
            deadlift: 0.0,
        };
        let maxes = form.day_maxes(&expected, 3.0, &own);
        assert!((maxes.squat - 210.0).abs() < 1e-9);
        assert!((maxes.bench - 190.0).abs() < 1e-9);
        assert!((maxes.deadlift - 200.0).abs() < 1e-9);
    }

    #[test]
    fn day_max_never_drops_to_zero() {
        let form = DayForm {
            spread: 0.5,
            correlation: 1.0,
        };
        let maxes = form.day_maxes(
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
                correlation: 0.5
            }
            .is_valid()
        );
        for (spread, correlation) in [
            (-0.01, 0.5),
            (0.6, 0.5),
            (0.03, 1.1),
            (0.03, -0.1),
            (f64::NAN, 0.5),
        ] {
            assert!(
                !DayForm {
                    spread,
                    correlation
                }
                .is_valid()
            );
        }
    }
}
