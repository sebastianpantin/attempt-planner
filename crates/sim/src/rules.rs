use std::fmt;

pub const ATTEMPTS_PER_LIFT: usize = 3;
pub const PLATE_STEP: Weight = Weight::from_grams(2_500);
pub const BAR: Weight = Weight::from_grams(20_000);
pub const HEAVIEST: Weight = Weight::from_grams(1_000_000);

/// A bar weight, stored in grams so comparisons and plate checks are exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Weight(u32);

impl Weight {
    pub const ZERO: Weight = Weight(0);

    pub const fn from_grams(grams: u32) -> Self {
        Self(grams)
    }

    /// Converts kilograms, rounding to the nearest gram. `None` for negative or non-finite input.
    pub fn from_kg(kg: f64) -> Option<Self> {
        let grams = (kg * 1000.0).round();
        (kg >= 0.0 && grams <= f64::from(u32::MAX)).then_some(Self(grams as u32))
    }

    pub const fn grams(self) -> u32 {
        self.0
    }

    pub fn kg(self) -> f64 {
        f64::from(self.0) / 1000.0
    }

    pub fn is_loadable(self) -> bool {
        self.0.is_multiple_of(PLATE_STEP.0)
    }
}

impl std::ops::Add for Weight {
    type Output = Weight;

    fn add(self, other: Weight) -> Weight {
        Weight(self.0.saturating_add(other.0))
    }
}

impl std::iter::Sum for Weight {
    fn sum<I: Iterator<Item = Weight>>(iter: I) -> Weight {
        iter.fold(Weight::ZERO, |total, weight| total + weight)
    }
}

impl fmt::Display for Weight {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} kg", self.kg())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attempt {
    pub weight: Weight,
    pub made: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleViolation {
    TooManyAttempts,
    BelowBar { attempt: usize },
    TooHeavy { attempt: usize },
    NotLoadable { attempt: usize },
    Lighter { attempt: usize },
    NoIncreaseAfterMake { attempt: usize },
}

impl fmt::Display for RuleViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyAttempts => write!(f, "a lift has at most {ATTEMPTS_PER_LIFT} attempts"),
            Self::BelowBar { attempt } => {
                write!(f, "attempt {attempt} is lighter than the {BAR} bar")
            }
            Self::TooHeavy { attempt } => {
                write!(f, "attempt {attempt} is heavier than {HEAVIEST}")
            }
            Self::NotLoadable { attempt } => {
                write!(f, "attempt {attempt} is not a multiple of {PLATE_STEP}")
            }
            Self::Lighter { attempt } => {
                write!(f, "attempt {attempt} is lighter than the one before it")
            }
            Self::NoIncreaseAfterMake { attempt } => {
                write!(
                    f,
                    "attempt {attempt} must be heavier than the made attempt before it"
                )
            }
        }
    }
}

impl std::error::Error for RuleViolation {}

/// Checks IPF-style attempt rules: weights start at the bar, go up in plate steps, never get lighter,
/// and must go up after a made attempt. A missed weight may be repeated.
pub fn check_progression(attempts: &[Attempt]) -> Result<(), RuleViolation> {
    if attempts.len() > ATTEMPTS_PER_LIFT {
        return Err(RuleViolation::TooManyAttempts);
    }
    let mut previous: Option<Attempt> = None;
    for (attempt, current) in (1..).zip(attempts) {
        if current.weight < BAR {
            return Err(RuleViolation::BelowBar { attempt });
        }
        if current.weight > HEAVIEST {
            return Err(RuleViolation::TooHeavy { attempt });
        }
        if !current.weight.is_loadable() {
            return Err(RuleViolation::NotLoadable { attempt });
        }
        if let Some(previous) = previous {
            if current.weight < previous.weight {
                return Err(RuleViolation::Lighter { attempt });
            }
            if previous.made && current.weight == previous.weight {
                return Err(RuleViolation::NoIncreaseAfterMake { attempt });
            }
        }
        previous = Some(*current);
    }
    Ok(())
}

/// The heaviest made attempt, or `None` when every attempt was missed (a bomb-out).
pub fn best_attempt(attempts: &[Attempt]) -> Option<Weight> {
    attempts.iter().filter(|a| a.made).map(|a| a.weight).max()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{made, missed};

    #[test]
    fn accepts_a_normal_progression() {
        let attempts = [made(200.0), made(210.0), missed(217.5)];
        assert_eq!(check_progression(&attempts), Ok(()));
    }

    #[test]
    fn allows_repeating_a_missed_weight() {
        let attempts = [missed(200.0), missed(200.0), made(200.0)];
        assert_eq!(check_progression(&attempts), Ok(()));
    }

    #[test]
    fn requires_an_increase_after_a_make() {
        let attempts = [made(200.0), made(200.0)];
        assert_eq!(
            check_progression(&attempts),
            Err(RuleViolation::NoIncreaseAfterMake { attempt: 2 })
        );
    }

    #[test]
    fn rejects_a_lighter_attempt() {
        let attempts = [missed(200.0), made(195.0)];
        assert_eq!(
            check_progression(&attempts),
            Err(RuleViolation::Lighter { attempt: 2 })
        );
    }

    #[test]
    fn rejects_weights_off_the_plate_step() {
        let attempts = [made(200.0), made(201.0)];
        assert_eq!(
            check_progression(&attempts),
            Err(RuleViolation::NotLoadable { attempt: 2 })
        );
    }

    #[test]
    fn rejects_weights_below_the_empty_bar() {
        assert_eq!(check_progression(&[made(20.0)]), Ok(()));
        assert_eq!(
            check_progression(&[made(17.5)]),
            Err(RuleViolation::BelowBar { attempt: 1 })
        );
        assert_eq!(
            RuleViolation::BelowBar { attempt: 1 }.to_string(),
            "attempt 1 is lighter than the 20 kg bar"
        );
    }

    #[test]
    fn rejects_attempts_over_the_heaviest_loadable_weight() {
        assert_eq!(check_progression(&[made(1_000.0)]), Ok(()));
        assert_eq!(
            check_progression(&[made(990.0), made(1_002.5)]),
            Err(RuleViolation::TooHeavy { attempt: 2 })
        );
        assert_eq!(
            RuleViolation::TooHeavy { attempt: 2 }.to_string(),
            "attempt 2 is heavier than 1000 kg"
        );
    }

    #[test]
    fn rejects_a_fourth_attempt() {
        let attempts = [100.0, 102.5, 105.0, 107.5].map(made);
        assert_eq!(
            check_progression(&attempts),
            Err(RuleViolation::TooManyAttempts)
        );
    }

    #[test]
    fn best_attempt_is_the_heaviest_make() {
        let attempts = [made(200.0), made(210.0), missed(217.5)];
        assert_eq!(best_attempt(&attempts), Weight::from_kg(210.0));
    }

    #[test]
    fn three_misses_bomb_out() {
        let attempts = [missed(200.0); 3];
        assert_eq!(best_attempt(&attempts), None);
    }

    #[test]
    fn kg_round_trips_through_grams() {
        assert_eq!(Weight::from_kg(217.5).map(Weight::kg), Some(217.5));
        assert_eq!(Weight::from_kg(0.1 + 0.2), Weight::from_kg(0.3));
    }

    #[test]
    fn rejects_unrepresentable_kg() {
        assert_eq!(Weight::from_kg(-2.5), None);
        assert_eq!(Weight::from_kg(-0.0004), None);
        assert_eq!(Weight::from_kg(f64::NAN), None);
        assert_eq!(Weight::from_kg(f64::INFINITY), None);
    }

    #[test]
    fn violations_read_as_sentences() {
        assert_eq!(
            RuleViolation::Lighter { attempt: 2 }.to_string(),
            "attempt 2 is lighter than the one before it"
        );
        assert_eq!(
            RuleViolation::NotLoadable { attempt: 1 }.to_string(),
            "attempt 1 is not a multiple of 2.5 kg"
        );
    }
}
