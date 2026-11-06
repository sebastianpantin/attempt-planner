//! Fits the model to the attempts lifters really took.
//!
//! Calibrating to make rates assumes lifters follow the typical jumps, but they choose
//! each attempt knowing how the day is going, so the make rates of a typical plan say
//! little about how fast the make rate falls near the day max. This fit instead scores
//! every attempt of the fitting lifters' latest meets as the backtest does: with maxes
//! from their past meets and the opener, and each 2nd and 3rd attempt as evidence of the
//! day max. It finds each lift's curve and load choice under which what happened is most
//! likely. Day form and the correlation between lifts stay as consecutive meets measure
//! them.

use attempt_sim::lift::{Lift, PerLift};
use attempt_sim::minimize::minimize_to;
use attempt_sim::model::MakeRateCurve;

use crate::attempts::{Estimates, Grid, LoadChoice, Meet, Tails, every_lift, sequential_chances};
use crate::backtest::Score;
use crate::fitted::Model;
use crate::parallel::{filter_map_in_parallel, in_parallel};

/// Lifters whose latest meets the fit scores, at most, spread evenly over the lifters.
const FIT_LIFTERS: usize = 3_000;
/// The maxes are estimated from past meets with the model fitted before, and then each
/// lift fitted in turn, until a round improves the log loss by less than this. The
/// estimates depend on the curves, so the first round, from the calibrated curves, is
/// never the last.
const LOG_LOSS_TOLERANCE: f64 = 1e-4;
/// Rounds at most, should the log loss keep moving.
const MOST_ROUNDS: usize = 6;
const TOLERANCE: f64 = 1e-6;
const MAX_EVALUATIONS: usize = 300;
/// Where the search for a load choice starts: below the day max, nearer to it after a
/// make, give or take 4%.
const FIRST_CHOICE: LoadChoice = LoadChoice {
    log_shares: [-0.05, -0.1, -0.01, -0.05],
    spread: 0.04,
};
/// Where the search for each heavier tail starts, when it is fitted.
const FIRST_DOF: f64 = 6.0;
const FIRST_FLOOR: f64 = 0.03;

/// Which of the heavier [`Tails`] the fit searches for. The rest stay normal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TailsToFit {
    pub day_form: bool,
    pub floor: bool,
    pub load_choice: bool,
}

impl TailsToFit {
    fn start(&self) -> Tails {
        let mut tails = Tails::NORMAL;
        if self.day_form {
            tails.day_form_dof = FIRST_DOF;
        }
        if self.floor {
            tails.floors = PerLift::from_fn(|_| FIRST_FLOOR);
        }
        if self.load_choice {
            tails.choice_dof = FIRST_DOF;
        }
        tails
    }
}

/// The model and load choices fitted to real attempts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttemptFit {
    pub model: Model,
    pub load_choices: PerLift<LoadChoice>,
    pub tails: Tails,
    /// Lifters whose latest meets the fit scored.
    pub lifters: usize,
    /// Mean log loss of their attempts under the fit.
    pub log_loss: f64,
}

/// A lifter's latest meet, with each lift's expected max from their meets before it and
/// the opener, which the fit leaves as it is: the typical openers stay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoredMeet {
    pub attempts: PerLift<[f64; 3]>,
    pub expected_kg: PerLift<f64>,
}

/// What the fit searches for: each lift's curve and load choice. The model's typical
/// openers and day form stay.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub model: Model,
    pub choices: PerLift<LoadChoice>,
    pub tails: Tails,
}

/// One lift's parameters as searched, so that every point is valid: the log steepness,
/// the square root of the technical-miss chance, the log shares of the load choice, its
/// log spread and the square root of the floor.
type LiftPoint = [f64; 8];
/// Each lift's search steps, in [`LiftPoint`] order.
const LIFT_STEPS: LiftPoint = [0.2, 0.05, 0.02, 0.02, 0.02, 0.02, 0.2, 0.1];

impl Params {
    fn point(&self, lift: Lift) -> LiftPoint {
        let curve = self.model.curves.get(lift);
        let choice = self.choices.get(lift);
        let [a, b, c, d] = choice.log_shares;
        [
            curve.steepness.ln(),
            curve.technical_miss_chance.sqrt(),
            a,
            b,
            c,
            d,
            choice.spread.ln(),
            self.tails.floors.get(lift).sqrt(),
        ]
    }

    fn with(&self, lift: Lift, point: LiftPoint) -> Self {
        let [steepness, root_chance, a, b, c, d, spread, root_floor] = point;
        let mut params = *self;
        *params.model.curves.get_mut(lift) = MakeRateCurve {
            ratio_at_half: 1.0,
            steepness: steepness.exp(),
            technical_miss_chance: (root_chance * root_chance).min(0.99),
        };
        *params.choices.get_mut(lift) = LoadChoice {
            log_shares: [a, b, c, d],
            spread: spread.exp(),
        };
        *params.tails.floors.get_mut(lift) = (root_floor * root_floor).min(0.99);
        params
    }

    /// Searches the first `N` of the lift's parameters near where they are, holding the
    /// rest.
    fn fit_lift<const N: usize>(&self, lift: Lift, meets: &[ScoredMeet]) -> Self {
        let full = self.point(lift);
        let with = |point: [f64; N]| {
            let mut full = full;
            full[..N].copy_from_slice(&point);
            self.with(lift, full)
        };
        let point = minimize_to(
            |point| log_loss(&with(point), meets),
            full[..N]
                .try_into()
                .expect("N is at most the point's length"),
            LIFT_STEPS[..N]
                .try_into()
                .expect("N is at most the steps' length"),
            TOLERANCE,
            MAX_EVALUATIONS,
        );
        with(point)
    }

    /// Searches one degree of freedom of the tails, by the log of how far it is above
    /// `least`, holding the rest.
    fn fit_dof(
        &self,
        meets: &[ScoredMeet],
        least: f64,
        dof: impl Fn(&mut Tails) -> &mut f64,
    ) -> Self {
        let with = |[log_excess]: [f64; 1]| {
            let mut params = *self;
            *dof(&mut params.tails) = least + log_excess.exp();
            params
        };
        let mut tails = self.tails;
        let start = (*dof(&mut tails) - least).ln();
        let point = minimize_to(
            |point| log_loss(&with(point), meets),
            [start],
            [0.3],
            TOLERANCE,
            MAX_EVALUATIONS,
        );
        with(point)
    }
}

/// Mean log loss of every attempt of `meets` under `params`.
pub fn log_loss(params: &Params, meets: &[ScoredMeet]) -> f64 {
    let model = &params.model;
    let draws = Grid::Fit.draws(params.tails.day_form_dof);
    in_parallel(meets, |meet| {
        let chances = sequential_chances(
            &draws,
            &model.curves,
            &meet.expected_kg,
            &model.day_form,
            &meet.attempts,
            Some(&params.choices),
            &params.tails,
        );
        let mut score = Score::default();
        for lift in Lift::ALL {
            for (&kg, &chance) in meet.attempts.get(lift).iter().zip(chances.get(lift)) {
                score.add(chance, kg > 0.0);
            }
        }
        score
    })
    .log_loss()
}

/// The parameters near `start` under which `meets` are most likely: each lift in turn,
/// holding the rest, as the lifts only meet through the shared part of the day, then each
/// tail in `tails` shared by the lifts. One pass is enough here, as fitting a lift barely
/// moves the best fit of the others; [`fit_to_attempts`] repeats it as the estimated maxes
/// move.
pub fn fit_to_meets(start: &Params, meets: &[ScoredMeet], tails: TailsToFit) -> Params {
    let mut params = *start;
    for lift in Lift::ALL {
        params = if tails.floor {
            params.fit_lift::<8>(lift, meets)
        } else {
            params.fit_lift::<7>(lift, meets)
        };
    }
    // A t with 2 degrees of freedom or fewer has no variance to scale to the day form's.
    if tails.day_form {
        params = params.fit_dof(meets, 2.0, |tails| &mut tails.day_form_dof);
    }
    if tails.load_choice {
        params = params.fit_dof(meets, 0.0, |tails| &mut tails.choice_dof);
    }
    params
}

/// Each lifter's latest meet with the expected maxes from their past meets and the opener,
/// leaving out lifters without an estimate for every lift.
fn scored_meets(model: &Model, lifters: &[&[Meet]]) -> Vec<ScoredMeet> {
    filter_map_in_parallel(lifters, |meets| {
        let (latest, past) = meets.split_last()?;
        let estimates = Estimates::of(model, latest, past)?;
        Some(ScoredMeet {
            attempts: latest.attempts,
            expected_kg: every_lift(&estimates.with_opener)?.map(|_, lift| lift.expected_max_kg),
        })
    })
}

/// Fits the model, load choices and `tails` to the latest meets of some of `lifters`, each
/// lifter's complete meets, starting from `start`, whose day form stays.
pub fn fit_to_attempts(start: &Model, lifters: &[Vec<Meet>], tails: TailsToFit) -> AttemptFit {
    let step = lifters.len().div_ceil(FIT_LIFTERS).max(1);
    let sample: Vec<&[Meet]> = lifters.iter().step_by(step).map(Vec::as_slice).collect();
    let mut params = Params {
        model: *start,
        choices: PerLift::from_fn(|_| FIRST_CHOICE),
        tails: tails.start(),
    };
    let mut best: Option<AttemptFit> = None;
    for _ in 0..MOST_ROUNDS {
        let meets = scored_meets(&params.model, &sample);
        params = fit_to_meets(&params, &meets, tails);
        let fit = AttemptFit {
            model: params.model,
            load_choices: params.choices,
            tails: params.tails,
            lifters: meets.len(),
            log_loss: log_loss(&params, &meets),
        };
        let best_loss = best.map_or(f64::INFINITY, |best| best.log_loss);
        // Not better by the tolerance, NaN included, ends the fit, keeping the best round.
        let improved = fit.log_loss < best_loss - LOG_LOSS_TOLERANCE;
        if fit.log_loss < best_loss {
            best = Some(fit);
        }
        if !improved {
            break;
        }
    }
    best.unwrap_or(AttemptFit {
        model: params.model,
        load_choices: params.choices,
        tails: params.tails,
        lifters: 0,
        log_loss: f64::NAN,
    })
}

#[cfg(test)]
mod tests {
    use attempt_sim::model::DayForm;
    use rand::{RngExt, SeedableRng};
    use rand_distr::StandardNormal;
    use rand_xoshiro::Xoshiro256PlusPlus;

    use super::*;
    use crate::attempts::{FIT_GRID, slots};

    const EXPECTED_KG: PerLift<f64> = PerLift {
        squat: 180.0,
        bench: 120.0,
        deadlift: 220.0,
    };

    fn truth() -> Params {
        let [steepness, technical_miss_chance] = [[30.0, 60.0, 45.0], [0.05, 0.02, 0.01]];
        Params {
            model: Model {
                curves: PerLift::from_fn(|lift| MakeRateCurve {
                    ratio_at_half: 1.0,
                    steepness: steepness[lift as usize],
                    technical_miss_chance: technical_miss_chance[lift as usize],
                }),
                typical_opener: PerLift::from_fn(|_| 0.9),
                day_form: DayForm {
                    spread: 0.03,
                    max_uncertainty: 0.02,
                    correlation: 0.4,
                },
            },
            choices: PerLift::from_fn(|_| LoadChoice {
                log_shares: [0.97, 0.95, 1.0, 0.98].map(f64::ln),
                spread: 0.02,
            }),
            tails: Tails::NORMAL,
        }
    }

    /// Meets of lifters whose past meets pin their expected max down, who open at the
    /// typical share of it and choose their 2nd and 3rd attempts from their day max as
    /// `params` say.
    fn simulated_meets(params: &Params, count: usize) -> Vec<ScoredMeet> {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(11);
        let model = &params.model;
        let weights = model.day_form.draw_weights();
        (0..count)
            .map(|_| {
                let shared: f64 = rng.sample(StandardNormal);
                let attempts = PerLift::from_fn(|lift| {
                    let expected_kg = *EXPECTED_KG.get(lift);
                    let day_max = weights.day_max(expected_kg, shared, rng.sample(StandardNormal));
                    let (curve, choice) = (model.curves.get(lift), params.choices.get(lift));
                    let mut attempts = [0.0; 3];
                    for attempt in 0..3 {
                        let load = if attempt == 0 {
                            model.typical_opener.get(lift) * expected_kg
                        } else {
                            let log_share = choice.log_shares[slots(&attempts)[attempt] - 1];
                            let z: f64 = rng.sample(StandardNormal);
                            day_max * (log_share + choice.spread * z).exp()
                        };
                        let made = rng.random::<f64>() < curve.make_probability_kg(load, day_max);
                        attempts[attempt] = if made { load } else { -load };
                    }
                    attempts
                });
                ScoredMeet {
                    attempts,
                    expected_kg: EXPECTED_KG,
                }
            })
            .collect()
    }

    /// Each attempt's chance of being made under `params`, given the attempts before it.
    fn chances(params: &Params, meet: &ScoredMeet) -> Vec<f64> {
        let model = &params.model;
        let chances = sequential_chances(
            &FIT_GRID,
            &model.curves,
            &meet.expected_kg,
            &model.day_form,
            &meet.attempts,
            Some(&params.choices),
            &params.tails,
        );
        Lift::ALL
            .iter()
            .flat_map(|&lift| *chances.get(lift))
            .collect()
    }

    // The curves and the load choices' spread trade off against each other in a sample
    // this size, so the test is the chances they give together.
    #[test]
    fn recovers_the_chances_and_load_choices_behind_simulated_attempts() {
        let truth = truth();
        let meets = simulated_meets(&truth, 800);
        let mut start = truth;
        for lift in Lift::ALL {
            let curve = start.model.curves.get_mut(lift);
            curve.steepness *= 1.5;
            curve.technical_miss_chance = 0.03;
        }
        start.choices = PerLift::from_fn(|_| FIRST_CHOICE);
        // Twice from so far off, as the fit does once for each estimate of the maxes.
        let tails = TailsToFit::default();
        let fit = fit_to_meets(&fit_to_meets(&start, &meets, tails), &meets, tails);
        assert!(log_loss(&fit, &meets) <= log_loss(&truth, &meets));
        let gaps: Vec<f64> = meets
            .iter()
            .flat_map(|meet| {
                let found = chances(&fit, meet);
                chances(&truth, meet)
                    .into_iter()
                    .zip(found)
                    .map(|(truth, found)| (truth - found).abs())
            })
            .collect();
        let mean_gap = gaps.iter().sum::<f64>() / gaps.len() as f64;
        assert!(mean_gap < 0.02, "{mean_gap} {fit:?}");
        // Few attempts follow a miss, so only the shares after a make are pinned down.
        for lift in Lift::ALL {
            let (found, truth) = (fit.choices.get(lift), truth.choices.get(lift));
            for slot in [0, 2] {
                let gap = found.log_shares[slot] - truth.log_shares[slot];
                assert!(gap.abs() < 0.02, "{lift} {fit:?}");
            }
        }
    }
}
