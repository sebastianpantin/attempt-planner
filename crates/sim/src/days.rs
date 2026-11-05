//! Simulated meet days stored as columns, one per lift and attempt, so a plan is scored by
//! a straight loop over arrays that the compiler turns into SIMD.

use crate::outcome::OutcomeTally;
use crate::plan::LiftPlan;
use crate::rules::{ATTEMPTS_PER_LIFT, HEAVIEST, PLATE_STEP, Weight};

/// A weight in plate steps. The attempts of a valid plan are whole plate steps, and even
/// three of the heaviest attempts add up to far less than `u16::MAX` steps.
pub(crate) type Steps = u16;

/// Rounds a part step down.
const fn whole_steps(weight: Weight) -> u32 {
    weight.grams() / PLATE_STEP.grams()
}

/// The heaviest valid attempt in plate steps.
pub(crate) const HEAVIEST_STEPS: usize = whole_steps(HEAVIEST) as usize;

/// A valid lift plan in plate steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StepPlan {
    opener: Steps,
    second_after_make: Steps,
    second_after_miss: Steps,
    third_after_make: Steps,
    third_after_miss: Steps,
}

impl StepPlan {
    /// The plan in plate steps. The plan must follow the attempt rules.
    pub(crate) fn new(plan: &LiftPlan) -> Self {
        debug_assert!(plan.problems().is_empty());
        let steps = |weight: Weight| whole_steps(weight) as Steps;
        Self {
            opener: steps(plan.opener),
            second_after_make: steps(plan.second.after_make),
            second_after_miss: steps(plan.second.after_miss),
            third_after_make: steps(plan.third.after_make),
            third_after_miss: steps(plan.third.after_miss),
        }
    }

    /// The best made attempt on one day, 0 when none is made, and which attempts are made as
    /// bit `i` for attempt `i`. Attempt `i` is made when its weight is below `missed_from[i]`.
    /// Written with selects only, so a loop over days becomes SIMD.
    #[inline(always)]
    fn day(&self, missed_from: [Steps; ATTEMPTS_PER_LIFT]) -> (Steps, u8) {
        let select = |made: bool, if_made: Steps, if_missed: Steps| {
            if made { if_made } else { if_missed }
        };
        let first = self.opener;
        let first_made = first < missed_from[0];
        let second = first + select(first_made, self.second_after_make, self.second_after_miss);
        let second_made = second < missed_from[1];
        let third = second + select(second_made, self.third_after_make, self.third_after_miss);
        let third_made = third < missed_from[2];
        let best = select(
            third_made,
            third,
            select(second_made, second, select(first_made, first, 0)),
        );
        let path = u8::from(first_made) | u8::from(second_made) << 1 | u8::from(third_made) << 2;
        (best, path)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LiftDays {
    /// For each attempt, each day's lightest weight in plate steps that is missed.
    missed_from: [Vec<Steps>; ATTEMPTS_PER_LIFT],
    /// For each day, bit `i` set when attempt `i` is a technical miss.
    technical: Vec<u8>,
}

impl LiftDays {
    pub(crate) fn with_capacity(days: usize) -> Self {
        Self {
            missed_from: std::array::from_fn(|_| Vec::with_capacity(days)),
            technical: Vec::with_capacity(days),
        }
    }

    /// Adds a day from each attempt's make threshold in kg, minus infinity for a technical miss.
    pub(crate) fn push(
        &mut self,
        thresholds_kg: [f64; ATTEMPTS_PER_LIFT],
        technical: [bool; ATTEMPTS_PER_LIFT],
    ) {
        for (column, threshold_kg) in self.missed_from.iter_mut().zip(thresholds_kg) {
            column.push(missed_from(threshold_kg));
        }
        self.technical.push(
            technical
                .iter()
                .enumerate()
                .map(|(attempt, &technical)| u8::from(technical) << attempt)
                .sum(),
        );
    }

    pub(crate) fn len(&self) -> usize {
        self.technical.len()
    }

    fn columns(&self, days: std::ops::Range<usize>) -> [&[Steps]; ATTEMPTS_PER_LIFT] {
        self.missed_from
            .each_ref()
            .map(|column| &column[days.clone()])
    }

    /// Writes the best made attempt of `plan` on each of `days` to `bests`, 0 for no make.
    pub(crate) fn bests(&self, plan: &StepPlan, days: std::ops::Range<usize>, bests: &mut [Steps]) {
        let [first, second, third] = self.columns(days);
        let limits = first.iter().zip(second).zip(third);
        for (best, ((&first, &second), &third)) in bests.iter_mut().zip(limits) {
            *best = plan.day([first, second, third]).0;
        }
    }

    /// Like [`bests`](Self::bests) over every day. Also counts each day's best in
    /// `best_counts`, indexed by plate steps, and returns how many days go each way, indexed
    /// by the attempts made (bits 0–2) and the technical misses (bits 3–5).
    pub(crate) fn outcomes(
        &self,
        plan: &StepPlan,
        bests: &mut [Steps],
        best_counts: &mut [u32],
    ) -> [u32; 64] {
        let [first, second, third] = self.columns(0..self.len());
        let days = first.iter().zip(second).zip(third).zip(&self.technical);
        let mut counts = [0; 64];
        for (best, (((&first, &second), &third), &technical)) in bests.iter_mut().zip(days) {
            let (made, path) = plan.day([first, second, third]);
            *best = made;
            best_counts[usize::from(made)] += 1;
            counts[usize::from(path | technical << ATTEMPTS_PER_LIFT)] += 1;
        }
        counts
    }

    /// Records each day's total of the best attempt of `plan` and `others`, the total of the
    /// other lifts on the same day with 0 for a bomb-out.
    pub(crate) fn record(
        &self,
        tally: &mut OutcomeTally,
        plan: &StepPlan,
        others: &[Steps],
        goal: Steps,
    ) {
        let [first, second, third] = self.columns(0..others.len());
        let days = first.iter().zip(second).zip(third).zip(others);
        let mut counts = *tally;
        for (((&first, &second), &third), &others) in days {
            let best = plan.day([first, second, third]).0;
            counts.record(sum_or_zero([best, others]), goal);
        }
        *tally = counts;
    }
}

/// The lightest whole number of plate steps that is not below `threshold_kg`, so a weight of
/// `k` steps is below the threshold exactly when `k` is below the result.
fn missed_from(threshold_kg: f64) -> Steps {
    let step_kg = PLATE_STEP.kg();
    let below = |steps: Steps| f64::from(steps) * step_kg < threshold_kg;
    let near = (threshold_kg / step_kg).clamp(0.0, f64::from(Steps::MAX - 1)) as Steps;
    if below(near) {
        near + 1
    } else if near > 0 && !below(near - 1) {
        near - 1
    } else {
        near
    }
}

/// The total of one day's bests, or 0 when any of them is 0.
#[inline(always)]
pub(crate) fn sum_or_zero<const N: usize>(bests: [Steps; N]) -> Steps {
    let all_made = bests.iter().all(|&best| best != 0);
    if all_made { bests.iter().sum() } else { 0 }
}

/// The goal in plate steps, at least 1 so a bomb-out never meets it. A total in whole
/// plate steps meets the goal exactly when it reaches this.
pub(crate) fn goal_steps(goal: Option<Weight>) -> Steps {
    goal.map_or(Steps::MAX, |goal| {
        let steps = goal.grams().div_ceil(PLATE_STEP.grams()).max(1);
        Steps::try_from(steps).unwrap_or(Steps::MAX)
    })
}

/// Records the total of each day's bests in three rows, one per lift.
pub(crate) fn record_rows(
    tally: &mut OutcomeTally,
    [first, second, third]: [&[Steps]; 3],
    goal: Steps,
) {
    let mut counts = *tally;
    for ((&first, &second), &third) in first.iter().zip(second).zip(third) {
        counts.record(sum_or_zero([first, second, third]), goal);
    }
    *tally = counts;
}

/// Each day's total of the best attempts of two lifts with their plans, 0 when either
/// has no make.
pub(crate) fn totals(
    [(first, first_plan), (second, second_plan)]: [(&LiftDays, StepPlan); 2],
) -> Vec<Steps> {
    let [a, b, c] = first.columns(0..first.len());
    let [d, e, f] = second.columns(0..first.len());
    a.iter()
        .zip(b)
        .zip(c)
        .zip(d.iter().zip(e).zip(f))
        .map(|(((&a, &b), &c), ((&d, &e), &f))| {
            sum_or_zero([first_plan.day([a, b, c]).0, second_plan.day([d, e, f]).0])
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Jumps;
    use crate::testing::kg;

    #[test]
    fn missed_from_splits_loads_exactly_at_the_threshold() {
        for threshold_kg in [100.0, 100.000_001, 99.999_999, 101.25, 0.0, -3.0] {
            let missed_from = missed_from(threshold_kg);
            for steps in 0..60 {
                let load_kg = f64::from(steps) * 2.5;
                assert_eq!(
                    steps < missed_from,
                    load_kg < threshold_kg,
                    "{threshold_kg} {steps}"
                );
            }
        }
        assert_eq!(missed_from(f64::NEG_INFINITY), 0);
        assert_eq!(missed_from(f64::INFINITY), Steps::MAX);
    }

    #[test]
    fn a_day_follows_the_jumps_and_keeps_the_heaviest_make() {
        let plan = StepPlan::new(&LiftPlan {
            opener: kg(100.0),
            second: Jumps {
                after_make: kg(10.0),
                after_miss: kg(0.0),
            },
            third: Jumps {
                after_make: kg(5.0),
                after_miss: kg(2.5),
            },
        });
        assert_eq!(plan.day([41, 45, 47]), (46, 0b111));
        assert_eq!(plan.day([41, 44, 47]), (45, 0b101));
        assert_eq!(plan.day([40, 41, 43]), (42, 0b110));
        assert_eq!(plan.day([0, 0, 0]), (0, 0));
    }

    #[test]
    fn the_goal_is_reached_by_the_first_whole_plate_step_at_or_above_it() {
        assert_eq!(goal_steps(Some(kg(500.0))), 200);
        assert_eq!(goal_steps(Some(kg(501.0))), 201);
        assert_eq!(goal_steps(Some(kg(0.0))), 1);
        assert_eq!(goal_steps(None), Steps::MAX);
    }
}
