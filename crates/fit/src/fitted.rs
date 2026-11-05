//! Every part of the planner's default model, fitted to one set of entries.

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::{DayForm, MakeRateCurve};

use crate::calibrate::{Calibration, OpenerMisses, calibrate_kinds, calibrate_lifts};
use crate::data::Entry;
use crate::history::{MeetToMeet, meet_to_meet, opener_misses};
use crate::stats::{AttemptStats, attempt_stats};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitted {
    pub stats: PerLift<AttemptStats>,
    pub calibrations: PerLift<Calibration>,
    pub history: MeetToMeet,
    pub opener_misses: PerLift<OpenerMisses>,
    /// Calibrated with a share of lifters prone to misses whatever the load.
    pub kinds: PerLift<Calibration>,
}

pub fn fit(entries: &[Entry]) -> Result<Fitted, String> {
    let [squat, bench, deadlift] = Lift::ALL.map(|lift| {
        attempt_stats(entries.iter().map(|entry| *entry.attempts.get(lift)))
            .ok_or(format!("not enough {lift} attempts"))
    });
    let stats = PerLift {
        squat: squat?,
        bench: bench?,
        deadlift: deadlift?,
    };
    let lifts = Lift::ALL.map(|lift| {
        let stats = stats.get(lift);
        (stats.make_rates, stats.jumps)
    });
    let calibrations = calibrate_lifts(&lifts);
    let history = meet_to_meet(entries).ok_or("no lifter has two meets to compare")?;
    let opener_misses = opener_misses(entries);
    let kinds = calibrate_kinds(&Lift::ALL.map(|lift| {
        let stats = stats.get(lift);
        (stats.make_rates, *opener_misses.get(lift), stats.jumps)
    }));
    Ok(Fitted {
        stats,
        calibrations: PerLift::from_fn(|lift| calibrations[lift as usize]),
        history,
        opener_misses,
        kinds: PerLift::from_fn(|lift| kinds[lift as usize]),
    })
}

/// A model to plan with: for each lift, the kinds of lifter as shares and make-rate
/// curves, with the typical opener and day form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub kinds: PerLift<[(f64, MakeRateCurve); 2]>,
    pub typical_opener: PerLift<f64>,
    pub day_form: DayForm,
}

impl Model {
    /// Each lift's curve with the technical-miss chance averaged over the kinds of lifter.
    pub fn mean_curves(&self) -> PerLift<MakeRateCurve> {
        self.kinds.map(|_, kinds| MakeRateCurve {
            technical_miss_chance: kinds
                .iter()
                .map(|(share, curve)| share * curve.technical_miss_chance)
                .sum(),
            ..kinds[0].1
        })
    }
}

impl Fitted {
    /// The spread of the day max that the make rates call for, both parts together.
    pub fn total_spread(&self) -> f64 {
        self.calibrations.squat.params.spread
    }

    /// Day form from lifters' consecutive meets, with the rest of a fitted spread put
    /// down to uncertainty in the expected max.
    fn day_form(&self, total_spread: f64) -> DayForm {
        let spread = self.history.mean_spread();
        DayForm {
            spread,
            max_uncertainty: (total_spread.powi(2) - spread.powi(2)).max(0.0).sqrt(),
            correlation: self.history.mean_correlation(),
        }
    }

    fn model_of(&self, calibrations: &PerLift<Calibration>) -> Model {
        Model {
            kinds: calibrations.map(|_, calibration| {
                let params = calibration.params;
                let curve = |technical_miss_chance| MakeRateCurve {
                    ratio_at_half: 1.0,
                    steepness: params.steepness,
                    technical_miss_chance,
                };
                [
                    (
                        1.0 - params.prone_share,
                        curve(params.technical_miss_chance),
                    ),
                    (
                        params.prone_share,
                        curve(params.prone_technical_miss_chance),
                    ),
                ]
            }),
            typical_opener: calibrations.map(|_, calibration| calibration.params.opener_ratio),
            day_form: self.day_form(calibrations.squat.params.spread),
        }
    }

    /// The planner's model: every lifter alike.
    pub fn model(&self) -> Model {
        self.model_of(&self.calibrations)
    }

    /// The model with a share of lifters prone to misses whatever the load.
    pub fn kinds_model(&self) -> Model {
        self.model_of(&self.kinds)
    }
}
