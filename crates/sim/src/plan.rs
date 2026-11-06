use crate::rules::{ATTEMPTS_PER_LIFT, Attempt, BAR, HEAVIEST, RuleViolation, Weight};

/// How far to jump for one attempt, depending on whether the previous attempt was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Jumps {
    pub after_make: Weight,
    pub after_miss: Weight,
}

/// Which part of a lift plan an input or problem belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanStep {
    Opener,
    Second,
    Third,
}

impl PlanStep {
    /// The steps in attempt order, so attempt `n` belongs to `ALL[n - 1]`.
    pub const ALL: [PlanStep; ATTEMPTS_PER_LIFT] = [Self::Opener, Self::Second, Self::Third];
}

/// The plan for one lift: an opener, then jump rules for the 2nd and 3rd attempts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LiftPlan {
    pub opener: Weight,
    pub second: Jumps,
    pub third: Jumps,
}

impl LiftPlan {
    fn later_attempts(&self) -> [(PlanStep, usize, Jumps); ATTEMPTS_PER_LIFT - 1] {
        [
            (PlanStep::Second, 2, self.second),
            (PlanStep::Third, 3, self.third),
        ]
    }

    /// Every way this plan could break the attempt rules, with the step it comes from.
    /// Empty when the plan is valid.
    pub fn problems(&self) -> Vec<(PlanStep, RuleViolation)> {
        let mut problems = Vec::new();
        if self.opener < BAR {
            problems.push((PlanStep::Opener, RuleViolation::BelowBar { attempt: 1 }));
        } else if !self.opener.is_loadable() {
            problems.push((PlanStep::Opener, RuleViolation::NotLoadable { attempt: 1 }));
        }
        let mut heaviest = self.opener;
        if heaviest > HEAVIEST {
            problems.push((PlanStep::Opener, RuleViolation::TooHeavy { attempt: 1 }));
        }
        for (step, attempt, jumps) in self.later_attempts() {
            heaviest = heaviest + jumps.after_make.max(jumps.after_miss);
            if heaviest > HEAVIEST {
                problems.push((step, RuleViolation::TooHeavy { attempt }));
            }
            if jumps.after_make == Weight::ZERO {
                problems.push((step, RuleViolation::NoIncreaseAfterMake { attempt }));
            }
            if !jumps.after_make.is_loadable() || !jumps.after_miss.is_loadable() {
                problems.push((step, RuleViolation::NotLoadable { attempt }));
            }
        }
        problems
    }

    /// The attempts taken on one meet day. Each attempt is made when its weight is below
    /// that attempt's make threshold (see `MakeRateCurve::make_threshold_kg`).
    pub fn attempts(
        &self,
        make_thresholds_kg: [f64; ATTEMPTS_PER_LIFT],
    ) -> [Attempt; ATTEMPTS_PER_LIFT] {
        let take = |weight: Weight, threshold_kg| Attempt {
            weight,
            made: weight.kg() < threshold_kg,
        };
        let mut attempts = [take(self.opener, make_thresholds_kg[0]); ATTEMPTS_PER_LIFT];
        for (index, (_, _, jumps)) in (1..).zip(self.later_attempts()) {
            let previous = attempts[index - 1];
            let jump = if previous.made {
                jumps.after_make
            } else {
                jumps.after_miss
            };
            attempts[index] = take(previous.weight + jump, make_thresholds_kg[index]);
        }
        attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MakeRateCurve;
    use crate::testing::{kg, made, missed};

    fn thresholds(curve: &MakeRateCurve, day_max_kg: f64, rolls: [f64; 3]) -> [f64; 3] {
        rolls.map(|roll| curve.make_threshold_kg(day_max_kg, roll))
    }

    fn plan() -> LiftPlan {
        LiftPlan {
            opener: kg(180.0),
            second: Jumps {
                after_make: kg(10.0),
                after_miss: kg(0.0),
            },
            third: Jumps {
                after_make: kg(7.5),
                after_miss: kg(2.5),
            },
        }
    }

    const SHARP: MakeRateCurve = MakeRateCurve {
        ratio_at_half: 1.0,
        steepness: 1_000.0,
        technical_miss_chance: 0.0,
    };

    #[test]
    fn a_normal_plan_has_no_problems() {
        assert_eq!(plan().problems(), []);
    }

    #[test]
    fn jumps_follow_each_outcome() {
        let all_made = plan().attempts(thresholds(&SHARP, 250.0, [0.5; 3]));
        assert_eq!(all_made, [made(180.0), made(190.0), made(197.5)]);
        let all_missed = plan().attempts(thresholds(&SHARP, 150.0, [0.5; 3]));
        assert_eq!(all_missed, [missed(180.0), missed(180.0), missed(182.5)]);
    }

    #[test]
    fn rolls_decide_each_attempt() {
        let curve = MakeRateCurve {
            ratio_at_half: 1.0,
            steepness: 20.0,
            technical_miss_chance: 0.0,
        };
        let attempts = plan().attempts(thresholds(&curve, 180.0, [0.4, 0.6, 0.0]));
        assert_eq!(attempts, [made(180.0), missed(190.0), made(192.5)]);
    }

    #[test]
    fn reports_every_problem_with_its_attempt() {
        let plan = LiftPlan {
            opener: kg(17.5),
            second: Jumps {
                after_make: kg(0.0),
                after_miss: kg(1.0),
            },
            third: Jumps {
                after_make: kg(3.0),
                after_miss: kg(0.0),
            },
        };
        assert_eq!(
            plan.problems(),
            [
                (PlanStep::Opener, RuleViolation::BelowBar { attempt: 1 }),
                (
                    PlanStep::Second,
                    RuleViolation::NoIncreaseAfterMake { attempt: 2 }
                ),
                (PlanStep::Second, RuleViolation::NotLoadable { attempt: 2 }),
                (PlanStep::Third, RuleViolation::NotLoadable { attempt: 3 }),
            ]
        );
    }

    #[test]
    fn reports_attempts_that_could_go_over_the_heaviest_weight() {
        let plan = LiftPlan {
            opener: kg(990.0),
            second: Jumps {
                after_make: kg(5.0),
                after_miss: kg(12.5),
            },
            third: Jumps {
                after_make: kg(2.5),
                after_miss: kg(0.0),
            },
        };
        assert_eq!(
            plan.problems(),
            [
                (PlanStep::Second, RuleViolation::TooHeavy { attempt: 2 }),
                (PlanStep::Third, RuleViolation::TooHeavy { attempt: 3 })
            ]
        );
    }

    #[test]
    fn opener_off_the_plate_step_is_not_loadable() {
        let plan = LiftPlan {
            opener: kg(181.0),
            ..plan()
        };
        assert_eq!(
            plan.problems(),
            [(PlanStep::Opener, RuleViolation::NotLoadable { attempt: 1 })]
        );
    }
}
