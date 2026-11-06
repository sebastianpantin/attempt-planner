//! Every part of the planner's default model, fitted to one set of entries.

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::model::{DayForm, MakeRateCurve};

use crate::attempts::lifters_meets;
use crate::calibrate::{Calibration, calibrate_lifts};
use crate::data::Entry;
use crate::history::{MeetToMeet, meet_to_meet};
use crate::likelihood::{AttemptFit, TailsToFit, fit_to_attempts};
use crate::progress::{ProgressFit, fit_progress};
use crate::stats::{AttemptStats, attempt_stats};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fitted {
    pub stats: PerLift<AttemptStats>,
    /// Calibrated to the make rates of typical plans: where the fit to real attempts
    /// starts, and the typical openers.
    pub calibrations: PerLift<Calibration>,
    pub history: MeetToMeet,
    /// How much stronger lifters get from meet to meet. The estimate of the maxes from past
    /// meets uses the progress shipped with the engine, [`PROGRESS`], not this.
    ///
    /// [`PROGRESS`]: attempt_sim::past_meets::PROGRESS
    pub progress: ProgressFit,
    pub attempts: AttemptFit,
}

/// Fits every part of the model to `entries`, with the heavier `tails` asked for.
pub fn fit(entries: &[Entry], tails: TailsToFit) -> Result<Fitted, String> {
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
    let calibrations = PerLift::from_fn(|lift| calibrations[lift as usize]);
    let history = meet_to_meet(entries).ok_or("no lifter has two meets to compare")?;
    let calibrated = calibrated_model(&calibrations, &history);
    let lifters = lifters_meets(entries);
    let progress =
        fit_progress(&lifters).ok_or("too few lifters with consecutive complete meets")?;
    Ok(Fitted {
        stats,
        calibrations,
        history,
        progress,
        attempts: fit_to_attempts(&calibrated, &lifters, tails),
    })
}

/// A model to plan with: each lift's make-rate curve, with the typical opener and day
/// form.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Model {
    pub curves: PerLift<MakeRateCurve>,
    pub typical_opener: PerLift<f64>,
    pub day_form: DayForm,
}

/// The model calibrated to make rates, with day form from lifters' consecutive meets and
/// the rest of the calibrated spread put down to uncertainty in the expected max.
fn calibrated_model(calibrations: &PerLift<Calibration>, history: &MeetToMeet) -> Model {
    let spread = history.mean_spread();
    let total_spread = calibrations.squat.params.spread;
    Model {
        curves: calibrations.map(|_, calibration| MakeRateCurve {
            ratio_at_half: 1.0,
            steepness: calibration.params.steepness,
            technical_miss_chance: calibration.params.technical_miss_chance,
        }),
        typical_opener: calibrations.map(|_, calibration| calibration.params.opener_ratio),
        day_form: DayForm {
            spread,
            max_uncertainty: (total_spread.powi(2) - spread.powi(2)).max(0.0).sqrt(),
            correlation: history.mean_correlation(),
        },
    }
}

impl Fitted {
    /// The spread of the day max that the make rates call for, both parts together.
    pub fn total_spread(&self) -> f64 {
        self.calibrations.squat.params.spread
    }

    /// The model calibrated to make rates.
    pub fn calibrated_model(&self) -> Model {
        calibrated_model(&self.calibrations, &self.history)
    }

    /// The planner's model, fitted to real attempts.
    pub fn model(&self) -> Model {
        self.attempts.model
    }
}
