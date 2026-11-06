//! Replanning during the meet. The attempts taken so far rule out the simulated days on
//! which they would have gone another way, and the search plans the rest of the meet on
//! the days that are left, so the chances it reports are given the meet so far.

use std::sync::Arc;

use rand::{RngExt, SeedableRng};
use rand_distr::{Distribution, StandardNormal};
use rand_xoshiro::Xoshiro256PlusPlus;

use crate::days::LiftDays;
use crate::evaluate::{InputProblem, Lifter, MeetDays, draw_attempts};
use crate::lift::{Lift, PerLift};
use crate::normal::normal_grid;
use crate::plan::{Jumps, LiftPlan, PlanStep};
use crate::rules::{
    ATTEMPTS_PER_LIFT, Attempt, PLATE_STEP, RuleViolation, Weight, check_progression,
};
use crate::search::{Found, Search, SearchStart};

/// Below this many simulated days that agree with the meet so far, the days are drawn
/// afresh. A chance near 50% on 2,000 days has a standard error of about 1.1 points, three
/// times that of the 20,000 days the page plans on, and the search, which keeps the best of
/// many plans scored on the same days, would start to pick plans for their luck.
pub const MIN_AGREEING_DAYS: usize = 2_000;

#[derive(Debug, Clone, PartialEq)]
pub struct MeetDayInput {
    pub lifter: Lifter,
    /// The plans going into the meet, which the replan starts from. Ignored when they
    /// break the attempt rules.
    pub plans: Option<PerLift<LiftPlan>>,
    pub goal: Option<Weight>,
    /// Highest acceptable chance of bombing out, if any.
    pub bomb_out_cap: Option<f64>,
    pub simulations: u32,
    pub seed: u64,
    /// Every attempt taken so far in meet order: the squats, the benches, then the
    /// deadlifts.
    pub taken: Vec<Attempt>,
}

/// The attempt to take next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextAttempt {
    pub lift: Lift,
    /// From 1.
    pub attempt: usize,
    pub weight: Weight,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Replan {
    /// Plans for the whole meet that take the attempts so far, and how they do given them.
    pub found: Found,
    /// Absent once every attempt is taken.
    pub next: Option<NextAttempt>,
    /// How many simulated meets the outcome comes from.
    pub meets: u32,
    /// Whether those meets were drawn afresh given the meet so far, because fewer than
    /// [`MIN_AGREEING_DAYS`] of the simulated days went the same way.
    pub redrawn: bool,
}

/// Plans the rest of the meet for the goal and bomb-out cap, given the attempts taken so
/// far. Only whether each attempt was made counts as evidence about the day: the loads
/// come from the plan, so a load says nothing about the day by itself.
pub fn replan(input: &MeetDayInput) -> Result<Replan, Vec<InputProblem>> {
    let days = MeetDays::shared_with_cap(
        input.lifter,
        input.simulations,
        input.seed,
        input.bomb_out_cap,
    );
    let taken = Taken::new(&input.taken);
    let (days, taken) = match (days, taken) {
        (Ok(days), Ok(taken)) => (days, taken),
        (days, taken) => {
            let mut problems = days.err().unwrap_or_default();
            problems.extend(taken.err().unwrap_or_default());
            return Err(problems);
        }
    };
    let lifts = taken.lifts();
    let (days, redrawn) = if input.taken.is_empty() {
        (days, false)
    } else if let Some(agreeing) = days.agreeing(&lifts, MIN_AGREEING_DAYS) {
        (Arc::new(agreeing), false)
    } else {
        let redrawn = redraw(&input.lifter, &lifts, input.simulations, input.seed);
        (Arc::new(redrawn), true)
    };
    let meets = days.simulations();
    let start = SearchStart {
        plans: input.plans,
        family_best: None,
        goal: input.goal,
        bomb_out_cap: input.bomb_out_cap,
    };
    let mut search = Search::on_days(days, taken.clone(), &start);
    while !search.is_done() {
        search.step();
    }
    let found = search.best();
    Ok(Replan {
        found,
        next: taken.next(&found.plans),
        meets,
        redrawn,
    })
}

/// The attempts taken so far at a meet, each lift's in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Taken(PerLift<Vec<Attempt>>);

impl Taken {
    /// Splits attempts in meet order by lift, checking each lift's against the attempt
    /// rules.
    fn new(in_meet_order: &[Attempt]) -> Result<Self, Vec<InputProblem>> {
        let mut problems = Vec::new();
        if in_meet_order.len() > Lift::ALL.len() * ATTEMPTS_PER_LIFT {
            problems.push(InputProblem::Plan {
                lift: Lift::Deadlift,
                step: PlanStep::Third,
                violation: RuleViolation::TooManyAttempts,
            });
        }
        let mut by_lift = in_meet_order.chunks(ATTEMPTS_PER_LIFT);
        let lifts = PerLift::from_fn(|lift| {
            let taken = by_lift.next().unwrap_or_default();
            if let Err(violation) = check_progression(taken) {
                let attempt = violation.attempt().unwrap_or(ATTEMPTS_PER_LIFT);
                problems.push(InputProblem::Plan {
                    lift,
                    step: PlanStep::ALL[attempt - 1],
                    violation,
                });
            }
            taken.to_vec()
        });
        if problems.is_empty() {
            Ok(Self(lifts))
        } else {
            Err(problems)
        }
    }

    fn lifts(&self) -> PerLift<&[Attempt]> {
        PerLift::from_fn(|lift| self.0.get(lift).as_slice())
    }

    /// `plan` changed to take the lift's attempts so far. A jump the meet can no longer
    /// take, after the result that did not happen, becomes the smallest valid one, so
    /// plans that differ only there are the same plan.
    pub(crate) fn pin(&self, lift: Lift, mut plan: LiftPlan) -> LiftPlan {
        let taken = self.0.get(lift);
        if let Some(opener) = taken.first() {
            plan.opener = opener.weight;
        }
        let unused = Jumps {
            after_make: PLATE_STEP,
            after_miss: Weight::ZERO,
        };
        for (index, jumps) in [&mut plan.second, &mut plan.third].into_iter().enumerate() {
            let Some(before) = taken.get(index) else {
                break;
            };
            let mut pinned = unused;
            let (slot, planned) = if before.made {
                (&mut pinned.after_make, jumps.after_make)
            } else {
                (&mut pinned.after_miss, jumps.after_miss)
            };
            *slot = taken.get(index + 1).map_or(planned, |after| {
                Weight::from_grams(after.weight.grams() - before.weight.grams())
            });
            *jumps = pinned;
        }
        plan
    }

    pub(crate) fn pin_all(&self, plans: &PerLift<LiftPlan>) -> PerLift<LiftPlan> {
        plans.map(|lift, &plan| self.pin(lift, plan))
    }

    /// The first attempt not yet taken, with its weight in `plans`.
    fn next(&self, plans: &PerLift<LiftPlan>) -> Option<NextAttempt> {
        let lift = Lift::ALL
            .into_iter()
            .find(|&lift| self.0.get(lift).len() < ATTEMPTS_PER_LIFT)?;
        let taken = self.0.get(lift);
        let results = std::array::from_fn(|index| match taken.get(index) {
            Some(attempt) if !attempt.made => f64::NEG_INFINITY,
            _ => f64::INFINITY,
        });
        let attempts = plans.get(lift).attempts(results);
        Some(NextAttempt {
            lift,
            attempt: taken.len() + 1,
            weight: attempts[taken.len()].weight,
        })
    }
}

/// Standard normal draws are taken from a grid of this many points from −6 to 6, 0.05
/// apart: fine enough that a day max moves by well under a plate across one step.
const GRID_POINTS: usize = 241;

/// `simulations` days drawn from the model given the attempts in `taken`.
///
/// A day max is a shared normal draw and a lift's own one (see `DayForm`). Their joint
/// chance given the attempts is worked out on a grid: for each shared draw, each lift's own
/// draws are weighted by the chance of that lift's attempts, and the shared draw by its
/// prior times each lift's total. A day picks a shared draw, then each lift's own draw
/// given it, each moved by up to half a grid step so day maxes do not cluster on the grid.
/// Lifts with no attempt yet draw their own part as usual. The attempts still to come are
/// rolled as for any day; those taken go as they went, a miss being technical as often as
/// it would be on that day max.
fn redraw(lifter: &Lifter, taken: &PerLift<&[Attempt]>, simulations: u32, seed: u64) -> MeetDays {
    let grid = normal_grid(GRID_POINTS, 6.0);
    let (nodes, ln_prior): (Vec<f64>, Vec<f64>) =
        grid.iter().map(|&(z, weight)| (z, weight.ln())).unzip();
    let n = nodes.len();
    let step = nodes[1] - nodes[0];
    let draws = lifter.day_form.draw_weights();
    let ln_own: PerLift<Option<Vec<f64>>> = lifter.lifts.map(|lift, model| {
        let taken = *taken.get(lift);
        (!taken.is_empty()).then(|| {
            let expected_kg = model.expected_max.kg();
            let ln_prior = &ln_prior;
            nodes
                .iter()
                .flat_map(|&shared| {
                    nodes.iter().zip(ln_prior).map(move |(&own, &ln_prior)| {
                        let day_max_kg = draws.day_max(expected_kg, shared, own);
                        taken.iter().fold(ln_prior, |sum, attempt| {
                            let load_kg = attempt.weight.kg();
                            sum + model
                                .curve
                                .ln_result_chance(load_kg, day_max_kg, attempt.made)
                        })
                    })
                })
                .collect()
        })
    });
    let ln_shared: Vec<f64> = (0..n)
        .map(|i| {
            let own = ln_own.iter().filter_map(|(_, rows)| rows.as_ref());
            own.fold(ln_prior[i], |sum, rows| {
                sum + ln_sum(&rows[i * n..(i + 1) * n])
            })
        })
        .collect();
    let shared_cumulative = cumulative(&ln_shared);
    let own_cumulative = ln_own.map(|_, rows| {
        rows.as_ref()
            .map(|rows| rows.chunks(n).flat_map(cumulative).collect::<Vec<_>>())
    });

    let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
    let jittered = |rng: &mut Xoshiro256PlusPlus, index: usize| {
        nodes[index] + step * (rng.random::<f64>() - 0.5)
    };
    let mut lifts = PerLift::from_fn(|_| LiftDays::with_capacity(simulations as usize));
    for _ in 0..simulations {
        let i = pick(&shared_cumulative, rng.random());
        let shared = jittered(&mut rng, i);
        for (lift, model) in lifter.lifts.iter() {
            let own = match own_cumulative.get(lift) {
                Some(rows) => {
                    let j = pick(&rows[i * n..(i + 1) * n], rng.random());
                    jittered(&mut rng, j)
                }
                None => StandardNormal.sample(&mut rng),
            };
            let day_max_kg = draws.day_max(model.expected_max.kg(), shared, own);
            let (mut thresholds_kg, mut technical) =
                draw_attempts(&mut rng, &model.curve, day_max_kg);
            let curve = &model.curve;
            for (index, attempt) in taken.get(lift).iter().enumerate() {
                technical[index] = !attempt.made && {
                    let ln_missed = curve.ln_result_chance(attempt.weight.kg(), day_max_kg, false);
                    let technical_share = (curve.technical_miss_chance.ln() - ln_missed).exp();
                    rng.random::<f64>() < technical_share
                };
                thresholds_kg[index] = if attempt.made {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                };
            }
            lifts.get_mut(lift).push(thresholds_kg, technical);
        }
    }
    MeetDays::from_lifts(lifts, *lifter, seed)
}

/// `ln` of the sum of weights given as logarithms, without overflow.
fn ln_sum(ln_weights: &[f64]) -> f64 {
    let highest = ln_weights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if highest == f64::NEG_INFINITY {
        return highest;
    }
    highest
        + ln_weights
            .iter()
            .map(|&w| (w - highest).exp())
            .sum::<f64>()
            .ln()
}

/// The running sums of weights given as logarithms, scaled so the largest weight is 1.
fn cumulative(ln_weights: &[f64]) -> Vec<f64> {
    let highest = ln_weights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    ln_weights
        .iter()
        .scan(0.0, |sum, &ln_weight| {
            *sum += (ln_weight - highest).exp();
            Some(*sum)
        })
        .collect()
}

/// The index that a uniform draw `u` from 0 to 1 picks from running sums of weights.
fn pick(cumulative: &[f64], u: f64) -> usize {
    let target = u * cumulative.last().copied().unwrap_or_default();
    cumulative
        .partition_point(|&sum| sum <= target)
        .min(cumulative.len() - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate::LiftModel;
    use crate::model::{DayForm, MakeRateCurve};
    use crate::search::Outcome;
    use crate::testing::{kg, made, missed};

    fn lifter() -> Lifter {
        let lift = |max: f64, steepness: f64| LiftModel {
            expected_max: kg(max),
            curve: MakeRateCurve {
                ratio_at_half: 1.0,
                steepness,
                technical_miss_chance: 0.01,
            },
        };
        Lifter {
            lifts: PerLift {
                squat: lift(200.0, 56.0),
                bench: lift(130.0, 94.0),
                deadlift: lift(240.0, 109.0),
            },
            day_form: DayForm {
                spread: 0.03,
                max_uncertainty: 0.03,
                correlation: 0.4,
            },
        }
    }

    fn plan(opener: f64, second: f64, third: f64) -> LiftPlan {
        let jumps = |after_make| Jumps {
            after_make: kg(after_make),
            after_miss: kg(0.0),
        };
        LiftPlan {
            opener: kg(opener),
            second: jumps(second),
            third: jumps(third),
        }
    }

    fn plans() -> PerLift<LiftPlan> {
        PerLift {
            squat: plan(180.0, 10.0, 7.5),
            bench: plan(117.5, 7.5, 5.0),
            deadlift: plan(215.0, 15.0, 10.0),
        }
    }

    fn input(taken: Vec<Attempt>) -> MeetDayInput {
        MeetDayInput {
            lifter: lifter(),
            plans: Some(plans()),
            goal: Some(kg(550.0)),
            bomb_out_cap: Some(0.05),
            simulations: 20_000,
            seed: 7,
            taken,
        }
    }

    fn outcome(taken: Vec<Attempt>) -> Outcome {
        replan(&input(taken)).unwrap().found.outcome
    }

    fn goal_chance(taken: Vec<Attempt>) -> f64 {
        outcome(taken).goal_probability.unwrap()
    }

    #[test]
    fn before_the_meet_it_is_the_search() {
        let replan = replan(&input(Vec::new())).unwrap();
        let searched = crate::search::search(&crate::search::SearchInput {
            lifter: lifter(),
            start: Some(plans()),
            goal: Some(kg(550.0)),
            bomb_out_cap: Some(0.05),
            simulations: 20_000,
            seed: 7,
            family_best: None,
        })
        .unwrap();
        assert_eq!(replan.found, searched);
        assert_eq!(replan.meets, 20_000);
        assert!(!replan.redrawn);
        assert_eq!(
            replan.next,
            Some(NextAttempt {
                lift: Lift::Squat,
                attempt: 1,
                weight: searched.plans.squat.opener,
            })
        );
    }

    #[test]
    fn a_made_opener_raises_the_chance_of_the_goal_and_a_miss_lowers_it() {
        let before = goal_chance(Vec::new());
        assert!(goal_chance(vec![made(185.0)]) > before);
        assert!(goal_chance(vec![missed(185.0)]) < before);
    }

    #[test]
    fn a_heavier_make_says_more_about_the_day_than_a_lighter_one() {
        assert!(goal_chance(vec![made(195.0)]) > goal_chance(vec![made(170.0)]));
    }

    #[test]
    fn the_plans_take_the_attempts_so_far() {
        let taken = vec![made(180.0), missed(192.5), made(192.5), missed(120.0)];
        let replan = replan(&input(taken)).unwrap();
        let squat = replan.found.plans.squat;
        assert_eq!(squat.opener, kg(180.0));
        assert_eq!(squat.second.after_make, kg(12.5));
        assert_eq!(squat.third.after_miss, kg(0.0));
        assert_eq!(replan.found.plans.bench.opener, kg(120.0));
        let next = replan.next.unwrap();
        assert_eq!((next.lift, next.attempt), (Lift::Bench, 2));
        assert_eq!(
            next.weight,
            kg(120.0) + replan.found.plans.bench.second.after_miss
        );
        for (_, plan) in replan.found.plans.iter() {
            assert_eq!(plan.problems(), []);
        }
    }

    #[test]
    fn a_finished_lift_counts_its_best_on_every_day() {
        let taken = vec![made(180.0), made(190.0), missed(200.0)];
        let replan = replan(&input(taken.clone())).unwrap();
        let evaluation = MeetDays::new(lifter(), 20_000, 7)
            .unwrap()
            .agreeing(&Taken::new(&taken).unwrap().lifts(), 0)
            .unwrap()
            .evaluate(&replan.found.plans, Some(kg(550.0)))
            .unwrap();
        assert_eq!(evaluation.lifts.squat.make_probability, 1.0);
        assert_eq!(evaluation.lifts.squat.median_best_kg, Some(190.0));
    }

    #[test]
    fn a_bombed_out_lift_leaves_no_chance_of_the_goal() {
        let outcome = outcome(vec![missed(180.0), missed(180.0), missed(180.0)]);
        assert_eq!(outcome.bomb_out_probability, 1.0);
        assert_eq!(outcome.goal_probability, Some(0.0));
        assert_eq!(outcome.expected_total_kg, 0.0);
    }

    #[test]
    fn every_attempt_taken_leaves_one_total() {
        let taken = vec![
            made(180.0),
            made(190.0),
            made(197.5),
            made(117.5),
            made(125.0),
            missed(130.0),
            made(215.0),
            made(230.0),
            made(240.0),
        ];
        let replan = replan(&input(taken)).unwrap();
        assert_eq!(replan.next, None);
        let outcome = replan.found.outcome;
        assert_eq!(outcome.expected_total_kg, 197.5 + 125.0 + 240.0);
        assert_eq!(outcome.goal_probability, Some(1.0));
    }

    #[test]
    fn unlikely_results_draw_the_days_afresh() {
        let taken = vec![missed(150.0), missed(150.0)];
        let replan = replan(&input(taken)).unwrap();
        assert!(replan.redrawn);
        assert_eq!(replan.meets, 20_000);
        assert!(replan.found.outcome.bomb_out_probability < 1.0);
        let likely = super::replan(&input(vec![made(180.0)])).unwrap();
        assert!(!likely.redrawn);
        assert!(likely.meets >= MIN_AGREEING_DAYS as u32);
    }

    #[test]
    fn redrawn_days_agree_with_the_filtered_ones() {
        let taken = Taken::new(&[missed(195.0)]).unwrap();
        let filtered = MeetDays::new(lifter(), 200_000, 3)
            .unwrap()
            .agreeing(&taken.lifts(), 0)
            .unwrap();
        let redrawn = redraw(&lifter(), &taken.lifts(), 200_000, 4);
        let evaluate = |days: &MeetDays| {
            days.evaluate(&taken.pin_all(&plans()), Some(kg(550.0)))
                .unwrap()
        };
        let (filtered, redrawn) = (evaluate(&filtered), evaluate(&redrawn));
        assert!((filtered.expected_total_kg - redrawn.expected_total_kg).abs() < 3.0);
        for lift in Lift::ALL {
            let chances = |evaluation: &crate::evaluate::Evaluation| {
                evaluation.lifts.get(lift).attempt_make_probabilities
            };
            for (a, b) in chances(&filtered).iter().zip(chances(&redrawn)) {
                assert!((a - b).abs() < 0.02, "{lift} {a} {b}");
            }
        }
    }

    #[test]
    fn redrawn_misses_are_technical_more_often_on_light_loads() {
        let technical_share = |load: f64| {
            let taken = [missed(load)];
            let days = redraw(&lifter(), &Taken::new(&taken).unwrap().lifts(), 20_000, 1);
            let plans = Taken::new(&taken).unwrap().pin_all(&plans());
            let squat = days.evaluate(&plans, None).unwrap().lifts.squat;
            squat.technical_miss_rate / (squat.technical_miss_rate + squat.strength_miss_rate)
        };
        assert!(technical_share(140.0) > 0.5);
        assert!(technical_share(210.0) < 0.1);
    }

    #[test]
    fn attempts_against_the_rules_are_refused() {
        let problems = replan(&input(vec![made(180.0), made(180.0)])).unwrap_err();
        assert_eq!(
            problems,
            [InputProblem::Plan {
                lift: Lift::Squat,
                step: PlanStep::Second,
                violation: RuleViolation::NoIncreaseAfterMake { attempt: 2 },
            }]
        );
        let problems = replan(&input(vec![made(180.0); 10])).unwrap_err();
        assert!(problems.contains(&InputProblem::Plan {
            lift: Lift::Deadlift,
            step: PlanStep::Third,
            violation: RuleViolation::TooManyAttempts,
        }));
    }

    #[test]
    fn pinning_keeps_the_attempts_taken_and_the_jumps_still_to_come() {
        let taken = Taken::new(&[made(180.0)]).unwrap();
        let mut plan = plans().squat;
        plan.second.after_miss = kg(5.0);
        let pinned = taken.pin(Lift::Squat, plan);
        assert_eq!(pinned.opener, kg(180.0));
        assert_eq!(pinned.second.after_make, plan.second.after_make);
        assert_eq!(pinned.second.after_miss, kg(0.0));
        assert_eq!(pinned.third, plan.third);
        assert_eq!(taken.pin(Lift::Bench, plans().bench), plans().bench);
    }

    #[test]
    fn picks_by_weight() {
        let sums = cumulative(&[0.0, f64::NEG_INFINITY, 0.0_f64]);
        assert_eq!(sums, [1.0, 1.0, 2.0]);
        assert_eq!(pick(&sums, 0.25), 0);
        assert_eq!(pick(&sums, 0.75), 2);
        assert_eq!(pick(&sums, 1.0), 2);
    }
}
