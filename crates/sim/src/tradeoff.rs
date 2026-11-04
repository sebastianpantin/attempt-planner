use std::collections::HashSet;
use std::ops::RangeInclusive;

use crate::evaluate::{InputProblem, Lifter, MeetDays};
use crate::lift::PerLift;
use crate::outcome::OutcomeTally;
use crate::plan::{Jumps, LiftPlan};
use crate::rules::{PLATE_STEP, Weight};

const OPENER_PERCENTS: RangeInclusive<u32> = 85..=95;
const SECOND_PERCENTS: RangeInclusive<u32> = 93..=101;
const THIRD_PERCENTS: RangeInclusive<u32> = 97..=107;

/// A plan for every lift given as percentages of each lift's expected max: the opener,
/// the 2nd attempt after a made opener and the 3rd attempt after a made 2nd.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct PlanShape {
    pub opener_percent: u32,
    pub second_percent: u32,
    pub third_percent: u32,
}

impl PlanShape {
    /// The plan for a lift with this expected max. Each attempt is its percentage of the
    /// max, rounded down to the plate step, and a miss repeats the weight. `None` when the
    /// rounded attempts break the attempt rules.
    pub fn plan(&self, expected_max: Weight) -> Option<LiftPlan> {
        let step = u64::from(PLATE_STEP.grams());
        let at = |percent: u32| {
            let grams = u64::from(expected_max.grams()) * u64::from(percent) / 100;
            u32::try_from(grams / step * step)
                .ok()
                .map(Weight::from_grams)
        };
        let opener = at(self.opener_percent)?;
        let second = at(self.second_percent)?;
        let third = at(self.third_percent)?;
        let jump = |from: Weight, to: Weight| {
            Some(Jumps {
                after_make: Weight::from_grams(to.grams().checked_sub(from.grams())?),
                after_miss: Weight::ZERO,
            })
        };
        let plan = LiftPlan {
            opener,
            second: jump(opener, second)?,
            third: jump(second, third)?,
        };
        plan.problems().is_empty().then_some(plan)
    }
}

/// Every shape the tradeoff sweeps, in 1% steps.
pub fn plan_family() -> impl Iterator<Item = PlanShape> {
    OPENER_PERCENTS.flat_map(|opener_percent| {
        SECOND_PERCENTS.flat_map(move |second_percent| {
            THIRD_PERCENTS.map(move |third_percent| PlanShape {
                opener_percent,
                second_percent,
                third_percent,
            })
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeoffInput {
    pub lifter: Lifter,
    pub goal: Option<Weight>,
    /// Highest acceptable chance of bombing out, from 0 to 1, for the best plan for the goal.
    pub bomb_out_cap: Option<f64>,
    pub simulations: u32,
    pub seed: u64,
}

/// How one shape from the family did over the simulated meets.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TradeoffPoint {
    pub shape: PlanShape,
    pub plans: PerLift<LiftPlan>,
    pub bomb_out_probability: f64,
    /// Average total, counting a bomb-out as 0.
    pub expected_total_kg: f64,
    /// Absent when no goal was given.
    pub goal_probability: Option<f64>,
    /// True when no other shape has a lower or equal bomb-out chance and a higher
    /// expected total: the best plan at this level of risk.
    pub on_frontier: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tradeoff {
    pub points: Vec<TradeoffPoint>,
    /// Index of the point within the bomb-out cap with the highest chance of reaching the
    /// goal, ties going to the higher expected total. Absent without a goal or without a
    /// point within the cap.
    pub best_for_goal: Option<usize>,
}

/// Scores every shape in the family that fits the lifter's maxes on the same simulated
/// meet days, so the differences between shapes are not noise. Shapes that round to the
/// same plans as an earlier shape are left out.
pub fn tradeoff(input: &TradeoffInput) -> Result<Tradeoff, Vec<InputProblem>> {
    let cap_is_valid = input
        .bomb_out_cap
        .is_none_or(|cap| (0.0..=1.0).contains(&cap));
    let days = match (
        MeetDays::new(input.lifter, input.simulations, input.seed),
        cap_is_valid,
    ) {
        (Ok(days), true) => days,
        (days, _) => {
            let mut problems = days.err().unwrap_or_default();
            if !cap_is_valid {
                problems.push(InputProblem::InvalidBombOutCap);
            }
            return Err(problems);
        }
    };
    let mut seen = HashSet::new();
    let candidates: Vec<(PlanShape, PerLift<LiftPlan>)> = plan_family()
        .filter_map(|shape| {
            let plan = |lift: &crate::evaluate::LiftModel| shape.plan(lift.expected_max);
            let lifts = &input.lifter.lifts;
            let plans = PerLift {
                squat: plan(&lifts.squat)?,
                bench: plan(&lifts.bench)?,
                deadlift: plan(&lifts.deadlift)?,
            };
            Some((shape, plans))
        })
        .filter(|(_, plans)| seen.insert(*plans))
        .collect();

    // Each lift has far fewer distinct plans than there are shapes, so every day scores
    // each distinct lift plan once and the shapes add up those results.
    let lift_plans: PerLift<Vec<LiftPlan>> = PerLift::from_fn(|lift| {
        let mut seen = HashSet::new();
        candidates
            .iter()
            .map(|(_, plans)| *plans.get(lift))
            .filter(|plan| seen.insert(*plan))
            .collect()
    });
    let plan_indices: Vec<PerLift<usize>> = candidates
        .iter()
        .map(|(_, plans)| {
            plans.map(|lift, plan| {
                lift_plans
                    .get(lift)
                    .iter()
                    .position(|p| p == plan)
                    .expect("every candidate plan is listed")
            })
        })
        .collect();

    let mut bests: PerLift<Vec<Option<Weight>>> = PerLift::from_fn(|_| Vec::new());
    let mut tallies = vec![OutcomeTally::default(); candidates.len()];
    for day in days.iter() {
        for (lift, plans) in lift_plans.iter() {
            let lift_bests = bests.get_mut(lift);
            lift_bests.clear();
            lift_bests.extend(plans.iter().map(|plan| day.best(lift, plan)));
        }
        for (indices, tally) in plan_indices.iter().zip(&mut tallies) {
            let total: Option<Weight> = indices
                .iter()
                .map(|(lift, &index)| bests.get(lift)[index])
                .sum();
            tally.record(total, input.goal);
        }
    }

    let mut points: Vec<TradeoffPoint> = candidates
        .into_iter()
        .zip(tallies)
        .map(|((shape, plans), tally)| {
            let outcome = tally.outcome(input.goal);
            TradeoffPoint {
                shape,
                plans,
                bomb_out_probability: outcome.bomb_out_probability,
                expected_total_kg: outcome.expected_total_kg,
                goal_probability: outcome.goal_probability,
                on_frontier: false,
            }
        })
        .collect();
    mark_frontier(&mut points);
    let best_for_goal = input
        .goal
        .and_then(|_| best_for_goal(&points, input.bomb_out_cap));
    Ok(Tradeoff {
        points,
        best_for_goal,
    })
}

fn mark_frontier(points: &mut [TradeoffPoint]) {
    let mut by_risk: Vec<usize> = (0..points.len()).collect();
    by_risk.sort_by(|&a, &b| {
        let (a, b) = (&points[a], &points[b]);
        a.bomb_out_probability
            .total_cmp(&b.bomb_out_probability)
            .then(b.expected_total_kg.total_cmp(&a.expected_total_kg))
    });
    let mut best_total_kg = f64::NEG_INFINITY;
    for index in by_risk {
        let point = &mut points[index];
        if point.expected_total_kg > best_total_kg {
            best_total_kg = point.expected_total_kg;
            point.on_frontier = true;
        }
    }
}

fn best_for_goal(points: &[TradeoffPoint], bomb_out_cap: Option<f64>) -> Option<usize> {
    let key = |point: &TradeoffPoint| {
        (
            point.goal_probability.unwrap_or(0.0),
            point.expected_total_kg,
        )
    };
    (0..points.len())
        .filter(|&index| bomb_out_cap.is_none_or(|cap| points[index].bomb_out_probability <= cap))
        .max_by(|&a, &b| {
            let ((goal_a, total_a), (goal_b, total_b)) = (key(&points[a]), key(&points[b]));
            goal_a.total_cmp(&goal_b).then(total_a.total_cmp(&total_b))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluate::LiftModel;
    use crate::model::{DayForm, MakeRateCurve};
    use crate::testing::kg;

    fn lifter(spread: f64) -> Lifter {
        let maxes = PerLift {
            squat: 200.0,
            bench: 130.0,
            deadlift: 240.0,
        };
        Lifter {
            lifts: maxes.map(|_, &max| LiftModel {
                expected_max: kg(max),
                curve: MakeRateCurve {
                    ratio_at_half: 1.0,
                    steepness: 30.0,
                    technical_miss_chance: 0.0,
                },
            }),
            day_form: DayForm {
                spread,
                max_uncertainty: 0.0,
                correlation: 0.5,
            },
        }
    }

    fn input(goal: Option<f64>) -> TradeoffInput {
        TradeoffInput {
            lifter: lifter(0.03),
            goal: goal.map(kg),
            bomb_out_cap: None,
            simulations: 1_000,
            seed: 3,
        }
    }

    fn shape(opener: u32, second: u32, third: u32) -> PlanShape {
        PlanShape {
            opener_percent: opener,
            second_percent: second,
            third_percent: third,
        }
    }

    #[test]
    fn shapes_round_each_attempt_down_to_the_plate_step() {
        assert_eq!(
            shape(90, 97, 102).plan(kg(200.0)),
            Some(LiftPlan {
                opener: kg(180.0),
                second: Jumps {
                    after_make: kg(12.5),
                    after_miss: kg(0.0),
                },
                third: Jumps {
                    after_make: kg(10.0),
                    after_miss: kg(0.0),
                },
            })
        );
    }

    #[test]
    fn shapes_need_room_for_every_attempt() {
        assert_eq!(shape(85, 93, 97).plan(kg(25.0)), None);
        assert_eq!(shape(85, 93, 97).plan(kg(22.5)), None);
        assert!(shape(85, 93, 97).plan(kg(100.0)).is_some());
    }

    #[test]
    fn the_family_sweeps_every_percentage() {
        let family: Vec<_> = plan_family().collect();
        assert_eq!(family.len(), 11 * 9 * 11);
        assert_eq!(family.first(), Some(&shape(85, 93, 97)));
        assert_eq!(family.last(), Some(&shape(95, 101, 107)));
    }

    #[test]
    fn every_point_matches_evaluating_its_plans() {
        let input = input(Some(540.0));
        let result = tradeoff(&input).unwrap();
        let days = MeetDays::new(input.lifter, input.simulations, input.seed).unwrap();
        for point in result.points.iter().step_by(97) {
            let evaluation = days.evaluate(&point.plans, input.goal).unwrap();
            let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
            assert!(close(
                point.bomb_out_probability,
                evaluation.bomb_out_probability
            ));
            assert!(close(point.expected_total_kg, evaluation.expected_total_kg));
            assert_eq!(point.goal_probability, evaluation.goal_probability);
        }
    }

    #[test]
    fn points_cover_each_distinct_plan_that_fits_every_lift_once() {
        let input = input(None);
        let points = tradeoff(&input).unwrap().points;
        let plans_for = |shape: PlanShape| {
            let lifts = &input.lifter.lifts;
            Some(PerLift {
                squat: shape.plan(lifts.squat.expected_max)?,
                bench: shape.plan(lifts.bench.expected_max)?,
                deadlift: shape.plan(lifts.deadlift.expected_max)?,
            })
        };
        let fitting: HashSet<_> = plan_family().filter_map(plans_for).collect();
        let swept: Vec<_> = points.iter().map(|p| p.plans).collect();
        assert_eq!(swept.len(), fitting.len());
        assert_eq!(swept.iter().copied().collect::<HashSet<_>>(), fitting);
        for point in &points {
            assert_eq!(plans_for(point.shape), Some(point.plans));
        }
    }

    #[test]
    fn the_frontier_is_exactly_the_plans_no_other_plan_beats() {
        let points = tradeoff(&input(None)).unwrap().points;
        let beats = |a: &TradeoffPoint, b: &TradeoffPoint| {
            a.bomb_out_probability <= b.bomb_out_probability
                && a.expected_total_kg >= b.expected_total_kg
                && (a.bomb_out_probability, a.expected_total_kg)
                    != (b.bomb_out_probability, b.expected_total_kg)
        };
        let frontier: Vec<_> = points.iter().filter(|p| p.on_frontier).collect();
        assert!(frontier.len() > 1);
        for point in &points {
            let beaten = points.iter().any(|other| beats(other, point));
            assert_eq!(point.on_frontier, !beaten, "{point:?}");
        }
    }

    #[test]
    fn the_best_plan_for_the_goal_has_the_highest_goal_probability() {
        let result = tradeoff(&input(Some(560.0))).unwrap();
        let best = &result.points[result.best_for_goal.unwrap()];
        for point in &result.points {
            assert!(point.goal_probability <= best.goal_probability);
            if point.goal_probability == best.goal_probability {
                assert!(point.expected_total_kg <= best.expected_total_kg);
            }
        }
    }

    #[test]
    fn the_best_plan_for_the_goal_stays_within_the_bomb_out_cap() {
        let uncapped = tradeoff(&input(Some(560.0))).unwrap();
        let riskiest_best = uncapped.points[uncapped.best_for_goal.unwrap()].bomb_out_probability;
        let cap = riskiest_best / 2.0;
        let result = tradeoff(&TradeoffInput {
            bomb_out_cap: Some(cap),
            ..input(Some(560.0))
        })
        .unwrap();
        assert_eq!(result.points, uncapped.points);
        let best = &result.points[result.best_for_goal.unwrap()];
        assert!(best.bomb_out_probability <= cap);
        for point in result
            .points
            .iter()
            .filter(|p| p.bomb_out_probability <= cap)
        {
            assert!(point.goal_probability <= best.goal_probability);
        }
    }

    #[test]
    fn no_plan_within_the_bomb_out_cap_means_no_best_plan_for_the_goal() {
        let mut input = input(Some(560.0));
        for lift in crate::lift::Lift::ALL {
            input.lifter.lifts.get_mut(lift).curve.technical_miss_chance = 0.3;
        }
        let safest = tradeoff(&input)
            .unwrap()
            .points
            .iter()
            .map(|p| p.bomb_out_probability)
            .fold(f64::INFINITY, f64::min);
        assert!(safest > 0.0);
        input.bomb_out_cap = Some(safest / 2.0);
        assert_eq!(tradeoff(&input).unwrap().best_for_goal, None);
    }

    #[test]
    fn without_a_goal_there_is_no_best_plan_for_it() {
        let result = tradeoff(&input(None)).unwrap();
        assert_eq!(result.best_for_goal, None);
        assert!(result.points.iter().all(|p| p.goal_probability.is_none()));
    }

    #[test]
    fn maxes_too_light_for_the_family_leave_no_points() {
        let mut input = input(Some(60.0));
        for lift in crate::lift::Lift::ALL {
            input.lifter.lifts.get_mut(lift).expected_max = kg(20.0);
        }
        let result = tradeoff(&input).unwrap();
        assert!(result.points.is_empty());
        assert_eq!(result.best_for_goal, None);
    }

    #[test]
    fn an_invalid_lifter_is_refused() {
        let mut input = input(None);
        input.lifter.day_form.spread = -1.0;
        input.simulations = 0;
        input.bomb_out_cap = Some(1.5);
        assert_eq!(
            tradeoff(&input).unwrap_err(),
            [
                InputProblem::InvalidDayForm,
                InputProblem::SimulationsOutOfRange,
                InputProblem::InvalidBombOutCap
            ]
        );
    }
}
