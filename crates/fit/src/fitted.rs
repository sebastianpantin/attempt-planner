//! Every part of the planner's default model, fitted to one set of entries.

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::{DayForm, MakeRateCurve};

use crate::calibrate::{Calibration, calibrate_lifts};
use crate::data::Entry;
use crate::history::{MeetToMeet, meet_to_meet};
use crate::stats::{AttemptStats, attempt_stats};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitted {
    pub stats: PerLift<AttemptStats>,
    pub calibrations: PerLift<Calibration>,
    pub history: MeetToMeet,
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
    Ok(Fitted {
        stats,
        calibrations: PerLift::from_fn(|lift| calibrations[lift as usize]),
        history,
    })
}

impl Fitted {
    /// The spread of the day max that the make rates call for, both parts together.
    pub fn total_spread(&self) -> f64 {
        self.calibrations.squat.params.spread
    }

    /// Day form from lifters' consecutive meets, with the rest of the fitted spread put
    /// down to uncertainty in the expected max.
    pub fn day_form(&self) -> DayForm {
        let spread = self.history.mean_spread();
        DayForm {
            spread,
            max_uncertainty: (self.total_spread().powi(2) - spread.powi(2))
                .max(0.0)
                .sqrt(),
            correlation: self.history.mean_correlation(),
        }
    }

    pub fn curves(&self) -> PerLift<MakeRateCurve> {
        self.calibrations.map(|_, calibration| MakeRateCurve {
            ratio_at_half: 1.0,
            steepness: calibration.params.steepness,
            technical_miss_chance: calibration.params.technical_miss_chance,
        })
    }

    pub fn typical_opener(&self) -> PerLift<f64> {
        self.calibrations
            .map(|_, calibration| calibration.params.opener_ratio)
    }
}
