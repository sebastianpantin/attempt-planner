use std::cell::RefCell;
use std::fmt;
use std::sync::Arc;

use rand::{RngExt, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::days::{HEAVIEST_STEPS, LiftDays, StepPlan, goal_steps, sum_or_zero};
use crate::lift::{Lift, PerLift};
use crate::model::{DayForm, MakeRateCurve};
use crate::outcome::is_valid_bomb_out_cap;
use crate::plan::{LiftPlan, PlanStep};
use crate::rules::{ATTEMPTS_PER_LIFT, BAR, PLATE_STEP, RuleViolation, Weight, best_attempt};

pub const MAX_SIMULATIONS: u32 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiftModel {
    /// The max the lifter expects to hit on a normal day at this meet.
    pub expected_max: Weight,
    pub curve: MakeRateCurve,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lifter {
    pub lifts: PerLift<LiftModel>,
    pub day_form: DayForm,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EvaluateInput {
    pub lifter: Lifter,
    pub plans: PerLift<LiftPlan>,
    pub goal: Option<Weight>,
    pub simulations: u32,
    pub seed: u64,
}

/// The input a problem belongs to, so a UI can mark it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum InputField {
    ExpectedMax,
    Curve,
    Opener,
    Second,
    Third,
    DayForm,
    Simulations,
    Goal,
    BombOutCap,
}

impl From<PlanStep> for InputField {
    fn from(step: PlanStep) -> Self {
        match step {
            PlanStep::Opener => Self::Opener,
            PlanStep::Second => Self::Second,
            PlanStep::Third => Self::Third,
        }
    }
}

/// A stable name for each kind of problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum ProblemCode {
    InvalidWeight,
    BelowBar,
    TooHeavy,
    NotLoadable,
    Lighter,
    NoIncreaseAfterMake,
    TooManyAttempts,
    ExpectedMaxBelowBar,
    InvalidCurve,
    InvalidDayForm,
    SimulationsOutOfRange,
    InvalidBombOutCap,
}

impl From<RuleViolation> for ProblemCode {
    fn from(violation: RuleViolation) -> Self {
        match violation {
            RuleViolation::TooManyAttempts => Self::TooManyAttempts,
            RuleViolation::BelowBar { .. } => Self::BelowBar,
            RuleViolation::TooHeavy { .. } => Self::TooHeavy,
            RuleViolation::NotLoadable { .. } => Self::NotLoadable,
            RuleViolation::Lighter { .. } => Self::Lighter,
            RuleViolation::NoIncreaseAfterMake { .. } => Self::NoIncreaseAfterMake,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputProblem {
    /// A weight given in kg that is negative or not a number.
    InvalidWeight {
        lift: Option<Lift>,
        field: InputField,
    },
    Plan {
        lift: Lift,
        step: PlanStep,
        violation: RuleViolation,
    },
    ExpectedMaxBelowBar {
        lift: Lift,
    },
    InvalidCurve {
        lift: Lift,
    },
    InvalidDayForm,
    SimulationsOutOfRange,
    /// A bomb-out cap that is not a probability between 0 and 1.
    InvalidBombOutCap,
}

impl InputProblem {
    pub fn lift(&self) -> Option<Lift> {
        match *self {
            Self::InvalidWeight { lift, .. } => lift,
            Self::Plan { lift, .. }
            | Self::ExpectedMaxBelowBar { lift }
            | Self::InvalidCurve { lift } => Some(lift),
            Self::InvalidDayForm | Self::SimulationsOutOfRange | Self::InvalidBombOutCap => None,
        }
    }

    pub fn field(&self) -> InputField {
        match *self {
            Self::InvalidWeight { field, .. } => field,
            Self::Plan { step, .. } => step.into(),
            Self::ExpectedMaxBelowBar { .. } => InputField::ExpectedMax,
            Self::InvalidCurve { .. } => InputField::Curve,
            Self::InvalidDayForm => InputField::DayForm,
            Self::SimulationsOutOfRange => InputField::Simulations,
            Self::InvalidBombOutCap => InputField::BombOutCap,
        }
    }

    pub fn code(&self) -> ProblemCode {
        match *self {
            Self::InvalidWeight { .. } => ProblemCode::InvalidWeight,
            Self::Plan { violation, .. } => violation.into(),
            Self::ExpectedMaxBelowBar { .. } => ProblemCode::ExpectedMaxBelowBar,
            Self::InvalidCurve { .. } => ProblemCode::InvalidCurve,
            Self::InvalidDayForm => ProblemCode::InvalidDayForm,
            Self::SimulationsOutOfRange => ProblemCode::SimulationsOutOfRange,
            Self::InvalidBombOutCap => ProblemCode::InvalidBombOutCap,
        }
    }
}

impl fmt::Display for InputProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidWeight { .. } => f.write_str("not a valid weight in kg"),
            Self::Plan { violation, .. } => violation.fmt(f),
            Self::ExpectedMaxBelowBar { .. } => {
                write!(f, "expected max is lighter than the {BAR} bar")
            }
            Self::InvalidCurve { .. } => {
                f.write_str("make-rate curve needs a positive, finite midpoint and steepness, and a technical-miss chance from 0 to below 1")
            }
            Self::InvalidDayForm => write!(
                f,
                "day-form spread and max uncertainty must be at least 0 and combine to at most {}, and correlation must be between 0 and 1",
                DayForm::MAX_SPREAD
            ),
            Self::SimulationsOutOfRange => {
                write!(f, "simulations must be between 1 and {MAX_SIMULATIONS}")
            }
            Self::InvalidBombOutCap => f.write_str("bomb-out cap must be a chance between 0 and 1"),
        }
    }
}

impl std::error::Error for InputProblem {}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct LiftSummary {
    /// Chance of at least one made attempt.
    pub make_probability: f64,
    /// Median best attempt over the meets where at least one attempt was made, so always a
    /// weight that can be loaded on the bar.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub median_best_kg: Option<f64>,
    /// Share of attempts missed because the load was too heavy on the day.
    pub strength_miss_rate: f64,
    /// Share of attempts missed whatever the load, for example on depth or a command.
    pub technical_miss_rate: f64,
    /// Chance of making each attempt, in order.
    pub attempt_make_probabilities: [f64; ATTEMPTS_PER_LIFT],
    /// Chance of missing all three attempts with at least one of them a technical miss.
    pub technical_bomb_out_probability: f64,
    /// Every way the three attempts can go, most likely first.
    pub paths: [AttemptPath; PATHS],
}

/// One way a lift's three attempts can go, and how often it happened.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct AttemptPath {
    /// Whether each attempt was made, in order.
    pub made: [bool; ATTEMPTS_PER_LIFT],
    /// The best made attempt on this path. Absent when every attempt is missed.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub best_kg: Option<f64>,
    pub share: f64,
}

/// How many ways three attempts can go.
pub const PATHS: usize = 1 << ATTEMPTS_PER_LIFT;

/// How often one total came up, counting a bomb-out as 0.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct TotalShare {
    pub total_kg: f64,
    pub share: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct Percentiles {
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
}

/// Monte Carlo standard errors of the matching estimates in [`Evaluation`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct StandardErrors {
    pub bomb_out_probability: f64,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub goal_probability: Option<f64>,
    pub expected_total_kg: f64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct Evaluation {
    pub bomb_out_probability: f64,
    /// Chance of a total at or above the goal. Absent when no goal was given.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub goal_probability: Option<f64>,
    /// Average total, counting a bomb-out as 0.
    pub expected_total_kg: f64,
    /// Average total over the meets without a bomb-out.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    #[cfg_attr(feature = "tsify", tsify(optional))]
    pub expected_total_without_bomb_out_kg: Option<f64>,
    /// Percentiles of the total, counting a bomb-out as 0.
    pub total_percentiles_kg: Percentiles,
    /// Every total that came up, from the lowest, with how often. A bomb-out is a total of 0.
    pub totals: Vec<TotalShare>,
    pub standard_errors: StandardErrors,
    pub lifts: PerLift<LiftSummary>,
}

/// A seeded set of simulated meet days for one lifter. Every plan evaluated against the
/// same `MeetDays` sees exactly the same days, so differences between plans are not noise.
#[derive(Debug, Clone, PartialEq)]
pub struct MeetDays {
    lifts: PerLift<LiftDays>,
    lifter: Lifter,
    simulations: u32,
    seed: u64,
}

impl MeetDays {
    pub fn new(lifter: Lifter, simulations: u32, seed: u64) -> Result<Self, Vec<InputProblem>> {
        let problems = setup_problems(&lifter, simulations);
        if !problems.is_empty() {
            return Err(problems);
        }
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let draws = lifter.day_form.draw_weights();
        let mut lifts = PerLift::from_fn(|_| LiftDays::with_capacity(simulations as usize));
        for _ in 0..simulations {
            let shared: f64 = StandardNormal.sample(&mut rng);
            for (lift, model) in lifter.lifts.iter() {
                let own = StandardNormal.sample(&mut rng);
                let day_max_kg = draws.day_max(model.expected_max.kg(), shared, own);
                let curve = &model.curve;
                let technical =
                    std::array::from_fn(|_| rng.random::<f64>() < curve.technical_miss_chance);
                let thresholds_kg = technical.map(|technical| {
                    let roll = rng.random();
                    if technical {
                        f64::NEG_INFINITY
                    } else {
                        curve.make_threshold_kg(day_max_kg, roll)
                    }
                });
                lifts.get_mut(lift).push(thresholds_kg, technical);
            }
        }
        Ok(Self {
            lifts,
            lifter,
            simulations,
            seed,
        })
    }

    /// Like [`new`](Self::new), but hands back the days from the last call on this thread
    /// when the lifter, simulations and seed are the same. Simulating the days is most of
    /// the work of scoring a plan, and a page asks again for the same lifter each time a
    /// plan changes.
    pub fn shared(
        lifter: Lifter,
        simulations: u32,
        seed: u64,
    ) -> Result<Arc<Self>, Vec<InputProblem>> {
        thread_local! {
            static LAST: RefCell<Option<Arc<MeetDays>>> = const { RefCell::new(None) };
        }
        LAST.with_borrow_mut(|last| {
            let same = |days: &&Arc<MeetDays>| {
                days.lifter == lifter && days.simulations == simulations && days.seed == seed
            };
            if let Some(days) = last.as_ref().filter(same) {
                return Ok(Arc::clone(days));
            }
            *last = None;
            let days = Arc::new(Self::new(lifter, simulations, seed)?);
            *last = Some(Arc::clone(&days));
            Ok(days)
        })
    }

    /// Like [`shared`](Self::shared), and also checks the bomb-out cap that goes with them.
    pub(crate) fn shared_with_cap(
        lifter: Lifter,
        simulations: u32,
        seed: u64,
        bomb_out_cap: Option<f64>,
    ) -> Result<Arc<Self>, Vec<InputProblem>> {
        let days = Self::shared(lifter, simulations, seed);
        if is_valid_bomb_out_cap(bomb_out_cap) {
            return days;
        }
        let mut problems = days.err().unwrap_or_default();
        problems.push(InputProblem::InvalidBombOutCap);
        Err(problems)
    }

    /// The lifter the days were simulated for.
    pub fn lifter(&self) -> &Lifter {
        &self.lifter
    }

    pub(crate) fn lifts(&self) -> &PerLift<LiftDays> {
        &self.lifts
    }

    /// How many days there are.
    pub(crate) fn len(&self) -> usize {
        self.simulations as usize
    }

    pub fn evaluate(
        &self,
        plans: &PerLift<LiftPlan>,
        goal: Option<Weight>,
    ) -> Result<Evaluation, Vec<InputProblem>> {
        let problems = plan_problems(plans);
        if !problems.is_empty() {
            return Err(problems);
        }
        let mut bests = PerLift::from_fn(|_| vec![0; self.len()]);
        let mut best_counts = PerLift::from_fn(|_| vec![0; HEAVIEST_STEPS + 1]);
        let outcomes = PerLift::from_fn(|lift| {
            let plan = StepPlan::new(plans.get(lift));
            let (bests, counts) = (bests.get_mut(lift), best_counts.get_mut(lift));
            self.lifts.get(lift).outcomes(&plan, bests, counts)
        });
        let mut totals = vec![0; ATTEMPTS_PER_LIFT * HEAVIEST_STEPS + 1];
        let days = bests.squat.iter().zip(&bests.bench).zip(&bests.deadlift);
        for ((&squat, &bench), &deadlift) in days {
            totals[usize::from(sum_or_zero([squat, bench, deadlift]))] += 1;
        }
        Ok(summarise(
            &totals,
            &best_counts,
            &outcomes,
            plans,
            goal,
            self.simulations,
        ))
    }
}

/// Simulates the meet `input.simulations` times and summarises the outcomes.
pub fn evaluate(input: &EvaluateInput) -> Result<Evaluation, Vec<InputProblem>> {
    let mut problems = setup_problems(&input.lifter, input.simulations);
    problems.extend(plan_problems(&input.plans));
    if !problems.is_empty() {
        return Err(problems);
    }
    MeetDays::shared(input.lifter, input.simulations, input.seed)?
        .evaluate(&input.plans, input.goal)
}

fn setup_problems(lifter: &Lifter, simulations: u32) -> Vec<InputProblem> {
    let mut problems = Vec::new();
    for (lift, model) in lifter.lifts.iter() {
        if model.expected_max < BAR {
            problems.push(InputProblem::ExpectedMaxBelowBar { lift });
        }
        if !model.curve.is_valid() {
            problems.push(InputProblem::InvalidCurve { lift });
        }
    }
    if !lifter.day_form.is_valid() {
        problems.push(InputProblem::InvalidDayForm);
    }
    if !(1..=MAX_SIMULATIONS).contains(&simulations) {
        problems.push(InputProblem::SimulationsOutOfRange);
    }
    problems
}

pub(crate) fn plan_problems(plans: &PerLift<LiftPlan>) -> Vec<InputProblem> {
    plans
        .iter()
        .flat_map(|(lift, plan)| {
            plan.problems()
                .into_iter()
                .map(move |(step, violation)| InputProblem::Plan {
                    lift,
                    step,
                    violation,
                })
        })
        .collect()
}

/// Bit `i` is set when attempt `i` was made.
const MADE: u8 = (1 << ATTEMPTS_PER_LIFT) - 1;

/// Summarises the counts of each total and of each lift's best attempt, both indexed by
/// plate steps with 0 for a bomb-out, and each lift's days counted by the attempts made
/// and missed technically (see [`LiftDays::outcomes`]).
fn summarise(
    totals: &[u32],
    best_counts: &PerLift<Vec<u32>>,
    outcomes: &PerLift<[u32; 64]>,
    plans: &PerLift<LiftPlan>,
    goal: Option<Weight>,
    simulations: u32,
) -> Evaluation {
    let runs = f64::from(simulations);
    let step_kg = PLATE_STEP.kg();
    let bomb_outs = totals[0];
    let (total_steps, total_steps_squared) =
        (0_u64..)
            .zip(totals)
            .fold((0, 0), |(sum, squares), (steps, &count)| {
                let count = u64::from(count);
                (sum + steps * count, squares + steps * steps * count)
            });
    let expected_total_kg = total_steps as f64 * step_kg / runs;
    let variance = (total_steps_squared as f64 * step_kg * step_kg / runs
        - expected_total_kg * expected_total_kg)
        .max(0.0);
    let completed = simulations - bomb_outs;

    let bomb_out_probability = f64::from(bomb_outs) / runs;
    let goal_probability = goal.map(|goal| {
        let from = usize::from(goal_steps(Some(goal)));
        let met: u32 = totals.get(from..).unwrap_or_default().iter().sum();
        f64::from(met) / runs
    });
    let proportion_error = |p: f64| (p * (1.0 - p) / runs).sqrt();
    let percentile = |quantile: f64| nearest_rank(totals, 0, quantile);
    let shares = (0_u32..)
        .zip(totals)
        .filter(|(_, count)| **count > 0)
        .map(|(steps, &count)| TotalShare {
            total_kg: f64::from(steps) * step_kg,
            share: f64::from(count) / runs,
        })
        .collect();

    Evaluation {
        bomb_out_probability,
        goal_probability,
        expected_total_kg,
        expected_total_without_bomb_out_kg: (completed > 0)
            .then(|| total_steps as f64 * step_kg / f64::from(completed)),
        total_percentiles_kg: Percentiles {
            p10: percentile(0.1),
            p50: percentile(0.5),
            p90: percentile(0.9),
        },
        totals: shares,
        standard_errors: StandardErrors {
            bomb_out_probability: proportion_error(bomb_out_probability),
            goal_probability: goal_probability.map(proportion_error),
            expected_total_kg: (variance / runs).sqrt(),
        },
        lifts: best_counts.map(|lift, best_counts| {
            let mut path_counts = [0; PATHS];
            let mut strength_misses = 0;
            let mut technical_misses = 0;
            let mut technical_bomb_outs = 0;
            for (outcome, &count) in (0_u8..).zip(outcomes.get(lift)) {
                let made = outcome & MADE;
                let technical = outcome >> ATTEMPTS_PER_LIFT;
                path_counts[usize::from(made)] += count;
                strength_misses += count * (!made & !technical & MADE).count_ones();
                technical_misses += count * technical.count_ones();
                if made == 0 && technical != 0 {
                    technical_bomb_outs += count;
                }
            }
            let attempts = runs * ATTEMPTS_PER_LIFT as f64;
            let paths = attempt_paths(plans.get(lift), &path_counts, runs);
            let made = simulations - best_counts[0];
            LiftSummary {
                make_probability: f64::from(made) / runs,
                median_best_kg: (made > 0).then(|| nearest_rank(best_counts, 1, 0.5)),
                strength_miss_rate: f64::from(strength_misses) / attempts,
                technical_miss_rate: f64::from(technical_misses) / attempts,
                attempt_make_probabilities: std::array::from_fn(|attempt| {
                    paths
                        .iter()
                        .filter(|p| p.made[attempt])
                        .map(|p| p.share)
                        .sum()
                }),
                technical_bomb_out_probability: f64::from(technical_bomb_outs) / runs,
                paths,
            }
        }),
    }
}

/// Every way the plan's three attempts can go, with how often each came up, most likely first.
fn attempt_paths(plan: &LiftPlan, counts: &[u32; PATHS], runs: f64) -> [AttemptPath; PATHS] {
    let mut paths: [AttemptPath; PATHS] = std::array::from_fn(|path| {
        let made = std::array::from_fn(|attempt| path & (1 << attempt) != 0);
        let certain = made.map(|made| {
            if made {
                f64::INFINITY
            } else {
                f64::NEG_INFINITY
            }
        });
        AttemptPath {
            made,
            best_kg: best_attempt(&plan.attempts(certain)).map(Weight::kg),
            share: f64::from(counts[path]) / runs,
        }
    });
    paths.sort_by(|a, b| b.share.total_cmp(&a.share));
    paths
}

/// The nearest-rank quantile in kg of the weights counted in `counts` by plate steps,
/// leaving out those under `from` steps. At least one weight must be counted from there.
fn nearest_rank(counts: &[u32], from: usize, quantile: f64) -> f64 {
    let counts = &counts[from..];
    let counted: u32 = counts.iter().sum();
    let rank = ((quantile * f64::from(counted)).ceil() as u32).clamp(1, counted);
    let mut seen = 0;
    let index = counts
        .iter()
        .position(|&count| {
            seen += count;
            seen >= rank
        })
        .expect("a weight is counted");
    (from + index) as f64 * PLATE_STEP.kg()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Jumps;
    use crate::rules::check_progression;
    use crate::testing::kg;
    use proptest::prelude::*;

    fn curve(steepness: f64) -> MakeRateCurve {
        MakeRateCurve {
            ratio_at_half: 1.0,
            steepness,
            technical_miss_chance: 0.0,
        }
    }

    const SHARP: f64 = 1_000.0;

    fn plan(opener: f64, second: f64, third: f64) -> LiftPlan {
        LiftPlan {
            opener: kg(opener),
            second: Jumps {
                after_make: kg(second),
                after_miss: kg(0.0),
            },
            third: Jumps {
                after_make: kg(third),
                after_miss: kg(0.0),
            },
        }
    }

    fn certain_meet() -> EvaluateInput {
        let maxes = PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        };
        EvaluateInput {
            lifter: Lifter {
                lifts: maxes.map(|_, &max| LiftModel {
                    expected_max: kg(max),
                    curve: curve(SHARP),
                }),
                day_form: DayForm {
                    spread: 0.0,
                    max_uncertainty: 0.0,
                    correlation: 0.5,
                },
            },
            plans: PerLift {
                squat: plan(180.0, 10.0, 15.0),
                bench: plan(117.5, 7.5, 7.5),
                deadlift: plan(215.0, 15.0, 5.0),
            },
            goal: None,
            simulations: 2_000,
            seed: 7,
        }
    }

    fn uncertain_meet(steepness: f64, spread: f64) -> EvaluateInput {
        let mut meet = certain_meet();
        meet.lifter.day_form.spread = spread;
        for lift in Lift::ALL {
            meet.lifter.lifts.get_mut(lift).curve = curve(steepness);
        }
        meet
    }

    #[test]
    fn certain_outcome_without_day_form_spread() {
        let result = evaluate(&certain_meet()).unwrap();
        assert_eq!(result.bomb_out_probability, 0.0);
        assert_eq!(result.lifts.squat.median_best_kg, Some(190.0));
        assert_eq!(result.lifts.bench.median_best_kg, Some(125.0));
        assert_eq!(result.lifts.deadlift.median_best_kg, Some(235.0));
        assert_eq!(result.expected_total_kg, 550.0);
        assert_eq!(result.expected_total_without_bomb_out_kg, Some(550.0));
        let p = result.total_percentiles_kg;
        assert_eq!((p.p10, p.p50, p.p90), (550.0, 550.0, 550.0));
        assert_eq!(result.standard_errors.expected_total_kg, 0.0);
        assert_eq!(result.standard_errors.bomb_out_probability, 0.0);
    }

    #[test]
    fn a_certain_day_has_one_total_and_one_path_per_lift() {
        let result = evaluate(&certain_meet()).unwrap();
        assert_eq!(
            result.totals,
            [TotalShare {
                total_kg: 550.0,
                share: 1.0
            }]
        );
        let squat = result.lifts.squat;
        assert_eq!(squat.attempt_make_probabilities, [1.0, 1.0, 0.0]);
        assert_eq!(
            squat.paths[0],
            AttemptPath {
                made: [true, true, false],
                best_kg: Some(190.0),
                share: 1.0
            }
        );
        let all_made = squat.paths.iter().find(|p| p.made == [true; 3]).unwrap();
        assert_eq!((all_made.best_kg, all_made.share), (Some(205.0), 0.0));
        let none_made = squat.paths.iter().find(|p| p.made == [false; 3]).unwrap();
        assert_eq!(none_made.best_kg, None);
    }

    #[test]
    fn a_bomb_out_is_a_total_of_zero() {
        let mut meet = certain_meet();
        meet.plans.bench = plan(140.0, 5.0, 5.0);
        let result = evaluate(&meet).unwrap();
        assert_eq!(
            result.totals,
            [TotalShare {
                total_kg: 0.0,
                share: 1.0
            }]
        );
        assert_eq!(result.lifts.bench.technical_bomb_out_probability, 0.0);
    }

    #[test]
    fn counts_bomb_outs_with_a_technical_miss() {
        let mut meet = certain_meet();
        meet.lifter.lifts.bench.curve.technical_miss_chance = 0.6;
        let bench = evaluate(&meet).unwrap().lifts.bench;
        let bomb_out = 1.0 - bench.make_probability;
        assert!((bomb_out - 0.6f64.powi(3)).abs() < 0.03, "{bench:?}");
        assert!((bench.technical_bomb_out_probability - bomb_out).abs() < 1e-9);
    }

    #[test]
    fn shares_add_up_and_agree_with_each_other() {
        let result = evaluate(&uncertain_meet(20.0, 0.05)).unwrap();
        let total_share: f64 = result.totals.iter().map(|t| t.share).sum();
        assert!((total_share - 1.0).abs() < 1e-9);
        assert!(
            result
                .totals
                .windows(2)
                .all(|w| w[0].total_kg < w[1].total_kg)
        );
        for (_, lift) in result.lifts.iter() {
            assert!(lift.paths.windows(2).all(|w| w[0].share >= w[1].share));
            let path_share: f64 = lift.paths.iter().map(|p| p.share).sum();
            assert!((path_share - 1.0).abs() < 1e-9);
            for attempt in 0..ATTEMPTS_PER_LIFT {
                let made: f64 = lift
                    .paths
                    .iter()
                    .filter(|p| p.made[attempt])
                    .map(|p| p.share)
                    .sum();
                assert!((made - lift.attempt_make_probabilities[attempt]).abs() < 1e-9);
            }
            let missed_all = lift.paths.iter().find(|p| p.made == [false; 3]).unwrap();
            assert!((missed_all.share - (1.0 - lift.make_probability)).abs() < 1e-9);
        }
    }

    #[test]
    fn goal_probability_compares_the_total() {
        let mut meet = certain_meet();
        assert_eq!(evaluate(&meet).unwrap().goal_probability, None);
        meet.goal = Some(kg(550.0));
        assert_eq!(evaluate(&meet).unwrap().goal_probability, Some(1.0));
        meet.goal = Some(kg(552.5));
        assert_eq!(evaluate(&meet).unwrap().goal_probability, Some(0.0));
    }

    #[test]
    fn opening_above_the_max_bombs_out() {
        let mut meet = certain_meet();
        meet.plans.bench = plan(140.0, 5.0, 5.0);
        meet.goal = Some(kg(100.0));
        let result = evaluate(&meet).unwrap();
        assert_eq!(result.bomb_out_probability, 1.0);
        assert_eq!(result.goal_probability, Some(0.0));
        assert_eq!(result.expected_total_kg, 0.0);
        assert_eq!(result.expected_total_without_bomb_out_kg, None);
        assert_eq!(result.lifts.bench.make_probability, 0.0);
        assert_eq!(result.lifts.bench.median_best_kg, None);
        assert_eq!(result.lifts.squat.make_probability, 1.0);
    }

    #[test]
    fn repeated_attempts_match_the_closed_form() {
        let mut meet = certain_meet();
        meet.lifter.lifts.squat.curve = curve(20.0);
        meet.plans.squat = plan(200.0, 2.5, 2.5);
        meet.simulations = 200_000;
        let expected = 1.0 - 0.5_f64.powi(3);
        let simulated = evaluate(&meet).unwrap().lifts.squat.make_probability;
        assert!(
            (simulated - expected).abs() < 0.005,
            "{simulated} vs {expected}"
        );
    }

    #[test]
    fn standard_errors_follow_the_binomial_formula() {
        let mut meet = uncertain_meet(25.0, 0.04);
        meet.goal = Some(kg(550.0));
        let result = evaluate(&meet).unwrap();
        let p = result.goal_probability.unwrap();
        let expected = (p * (1.0 - p) / f64::from(meet.simulations)).sqrt();
        assert!(p > 0.0 && p < 1.0);
        assert_eq!(result.standard_errors.goal_probability, Some(expected));
        assert!(result.standard_errors.expected_total_kg > 0.0);
    }

    #[test]
    fn more_simulations_shrink_the_standard_error() {
        let mut meet = uncertain_meet(25.0, 0.04);
        let few = evaluate(&meet).unwrap().standard_errors.expected_total_kg;
        meet.simulations *= 16;
        let many = evaluate(&meet).unwrap().standard_errors.expected_total_kg;
        assert!(many < few / 2.0, "{many} vs {few}");
    }

    #[test]
    fn same_seed_same_result_and_new_seed_new_result() {
        let mut meet = uncertain_meet(25.0, 0.04);
        let first = evaluate(&meet).unwrap();
        assert_eq!(evaluate(&meet).unwrap(), first);
        meet.seed = 8;
        assert_ne!(evaluate(&meet).unwrap(), first);
    }

    #[test]
    fn every_plan_sees_the_same_meet_days() {
        let meet = uncertain_meet(25.0, 0.04);
        let days = MeetDays::new(meet.lifter, meet.simulations, meet.seed).unwrap();
        let mut bolder = meet.plans;
        bolder.squat = plan(190.0, 7.5, 5.0);
        let first = days.evaluate(&meet.plans, None).unwrap();
        let second = days.evaluate(&bolder, None).unwrap();
        assert_ne!(first.lifts.squat, second.lifts.squat);
        assert_eq!(first.lifts.bench, second.lifts.bench);
        assert_eq!(first.lifts.deadlift, second.lifts.deadlift);
        assert_eq!(days.evaluate(&meet.plans, None), evaluate(&meet));
    }

    #[test]
    fn shared_days_are_reused_only_for_the_same_lifter_simulations_and_seed() {
        let meet = uncertain_meet(25.0, 0.04);
        let shared =
            |lifter, simulations, seed| MeetDays::shared(lifter, simulations, seed).unwrap();
        let days = shared(meet.lifter, meet.simulations, meet.seed);
        assert_eq!(
            *days,
            MeetDays::new(meet.lifter, meet.simulations, meet.seed).unwrap()
        );
        assert!(Arc::ptr_eq(
            &days,
            &shared(meet.lifter, meet.simulations, meet.seed)
        ));
        let mut stronger = meet.lifter;
        stronger.lifts.squat.expected_max = kg(205.0);
        for other in [
            shared(stronger, meet.simulations, meet.seed),
            shared(meet.lifter, meet.simulations + 1, meet.seed),
            shared(meet.lifter, meet.simulations, meet.seed + 1),
        ] {
            assert!(!Arc::ptr_eq(&days, &other));
        }
    }

    #[test]
    fn technical_misses_happen_at_their_chance_even_on_light_attempts() {
        let mut meet = certain_meet();
        meet.simulations = 20_000;
        meet.lifter.lifts.squat.curve.technical_miss_chance = 0.1;
        meet.plans.squat = plan(180.0, 10.0, 7.5);
        let squat = evaluate(&meet).unwrap().lifts.squat;
        assert_eq!(squat.strength_miss_rate, 0.0);
        assert!((squat.technical_miss_rate - 0.1).abs() < 0.01, "{squat:?}");
        assert!(
            (squat.make_probability - (1.0 - 0.001)).abs() < 0.001,
            "{squat:?}"
        );
    }

    #[test]
    fn attempts_over_the_day_max_are_strength_misses() {
        let mut meet = certain_meet();
        meet.plans.bench = plan(140.0, 2.5, 2.5);
        let bench = evaluate(&meet).unwrap().lifts.bench;
        assert_eq!(bench.strength_miss_rate, 1.0);
        assert_eq!(bench.technical_miss_rate, 0.0);
    }

    #[test]
    fn median_best_is_a_loadable_weight() {
        let result = evaluate(&uncertain_meet(25.0, 0.04)).unwrap();
        for (_, lift) in result.lifts.iter() {
            let best = Weight::from_kg(lift.median_best_kg.unwrap()).unwrap();
            assert!(best.is_loadable(), "{best}");
        }
    }

    #[test]
    fn correlated_lifts_spread_the_total_wider() {
        let mut meet = uncertain_meet(30.0, 0.05);
        meet.simulations = 20_000;
        let mut width = |correlation| {
            meet.lifter.day_form.correlation = correlation;
            let p = evaluate(&meet).unwrap().total_percentiles_kg;
            p.p90 - p.p10
        };
        assert!(width(1.0) > width(0.0));
    }

    #[test]
    fn invalid_input_lists_every_problem() {
        let mut meet = certain_meet();
        meet.plans.squat.opener = kg(181.0);
        meet.lifter.lifts.bench.expected_max = kg(15.0);
        meet.lifter.lifts.deadlift.curve.steepness = 0.0;
        meet.lifter.day_form.correlation = 2.0;
        meet.simulations = 0;
        assert_eq!(
            evaluate(&meet).unwrap_err(),
            [
                InputProblem::ExpectedMaxBelowBar { lift: Lift::Bench },
                InputProblem::InvalidCurve {
                    lift: Lift::Deadlift
                },
                InputProblem::InvalidDayForm,
                InputProblem::SimulationsOutOfRange,
                InputProblem::Plan {
                    lift: Lift::Squat,
                    step: PlanStep::Opener,
                    violation: RuleViolation::NotLoadable { attempt: 1 }
                },
            ]
        );
    }

    #[test]
    fn problems_name_their_lift_field_and_code() {
        let problems = [
            InputProblem::InvalidWeight {
                lift: None,
                field: InputField::Goal,
            },
            InputProblem::Plan {
                lift: Lift::Bench,
                step: PlanStep::Third,
                violation: RuleViolation::NoIncreaseAfterMake { attempt: 3 },
            },
            InputProblem::ExpectedMaxBelowBar {
                lift: Lift::Deadlift,
            },
            InputProblem::InvalidCurve { lift: Lift::Squat },
            InputProblem::InvalidDayForm,
            InputProblem::SimulationsOutOfRange,
        ];
        let described = problems.map(|p| (p.lift(), p.field(), p.code()));
        assert_eq!(
            described,
            [
                (None, InputField::Goal, ProblemCode::InvalidWeight),
                (
                    Some(Lift::Bench),
                    InputField::Third,
                    ProblemCode::NoIncreaseAfterMake
                ),
                (
                    Some(Lift::Deadlift),
                    InputField::ExpectedMax,
                    ProblemCode::ExpectedMaxBelowBar
                ),
                (
                    Some(Lift::Squat),
                    InputField::Curve,
                    ProblemCode::InvalidCurve
                ),
                (None, InputField::DayForm, ProblemCode::InvalidDayForm),
                (
                    None,
                    InputField::Simulations,
                    ProblemCode::SimulationsOutOfRange
                ),
            ]
        );
    }

    #[test]
    fn problems_read_as_sentences() {
        let messages = [
            InputProblem::InvalidWeight {
                lift: Some(Lift::Squat),
                field: InputField::Opener,
            },
            InputProblem::Plan {
                lift: Lift::Squat,
                step: PlanStep::Opener,
                violation: RuleViolation::BelowBar { attempt: 1 },
            },
            InputProblem::ExpectedMaxBelowBar { lift: Lift::Bench },
            InputProblem::InvalidCurve { lift: Lift::Bench },
            InputProblem::InvalidDayForm,
            InputProblem::SimulationsOutOfRange,
        ]
        .map(|problem| problem.to_string());
        assert_eq!(
            messages,
            [
                "not a valid weight in kg",
                "attempt 1 is lighter than the 20 kg bar",
                "expected max is lighter than the 20 kg bar",
                "make-rate curve needs a positive, finite midpoint and steepness, and a technical-miss chance from 0 to below 1",
                "day-form spread and max uncertainty must be at least 0 and combine to at most 0.5, and correlation must be between 0 and 1",
                "simulations must be between 1 and 1000000",
            ]
        );
    }

    #[test]
    fn too_many_simulations_are_refused() {
        let mut meet = certain_meet();
        meet.simulations = MAX_SIMULATIONS + 1;
        assert_eq!(
            evaluate(&meet).unwrap_err(),
            [InputProblem::SimulationsOutOfRange]
        );
    }

    fn steps(range: std::ops::RangeInclusive<u32>) -> impl Strategy<Value = Weight> {
        range.prop_map(|n| Weight::from_grams(n * PLATE_STEP.grams()))
    }

    fn valid_plan() -> impl Strategy<Value = LiftPlan> {
        (
            steps(8..=120),
            steps(1..=10),
            steps(0..=4),
            steps(1..=10),
            steps(0..=4),
        )
            .prop_map(|(opener, make2, miss2, make3, miss3)| LiftPlan {
                opener,
                second: Jumps {
                    after_make: make2,
                    after_miss: miss2,
                },
                third: Jumps {
                    after_make: make3,
                    after_miss: miss3,
                },
            })
    }

    proptest! {
        #[test]
        fn simulated_attempts_follow_the_rules(
            plan in valid_plan(),
            day_max_kg in 20.0..400.0f64,
            rolls in prop::array::uniform3(0.0..1.0f64),
        ) {
            prop_assert!(plan.problems().is_empty());
            let thresholds = rolls.map(|roll| curve(SHARP).make_threshold_kg(day_max_kg, roll));
            let attempts = plan.attempts(thresholds);
            prop_assert_eq!(check_progression(&attempts), Ok(()));
        }

        #[test]
        fn probabilities_stay_between_zero_and_one(
            squat in valid_plan(),
            bench in valid_plan(),
            deadlift in valid_plan(),
            spread in 0.0..0.2f64,
            correlation in 0.0..=1.0f64,
            seed in any::<u64>(),
        ) {
            let mut meet = uncertain_meet(20.0, spread);
            meet.lifter.day_form.correlation = correlation;
            meet.plans = PerLift { squat, bench, deadlift };
            meet.goal = Some(kg(500.0));
            meet.simulations = 200;
            meet.seed = seed;
            let result = evaluate(&meet).unwrap();
            for p in [result.bomb_out_probability, result.goal_probability.unwrap()] {
                prop_assert!((0.0..=1.0).contains(&p));
            }
            let p = result.total_percentiles_kg;
            prop_assert!(p.p10 <= p.p50 && p.p50 <= p.p90);
        }
    }
}
