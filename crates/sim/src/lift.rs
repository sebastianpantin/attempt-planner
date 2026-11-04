use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub enum Lift {
    Squat,
    Bench,
    Deadlift,
}

impl Lift {
    pub const ALL: [Lift; 3] = [Lift::Squat, Lift::Bench, Lift::Deadlift];
}

impl fmt::Display for Lift {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Lift::Squat => "squat",
            Lift::Bench => "bench",
            Lift::Deadlift => "deadlift",
        })
    }
}

/// One value per lift, in meet order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "tsify", derive(tsify::Tsify))]
pub struct PerLift<T> {
    pub squat: T,
    pub bench: T,
    pub deadlift: T,
}

impl<T> PerLift<T> {
    pub fn from_fn(mut f: impl FnMut(Lift) -> T) -> Self {
        Self {
            squat: f(Lift::Squat),
            bench: f(Lift::Bench),
            deadlift: f(Lift::Deadlift),
        }
    }

    pub fn get(&self, lift: Lift) -> &T {
        match lift {
            Lift::Squat => &self.squat,
            Lift::Bench => &self.bench,
            Lift::Deadlift => &self.deadlift,
        }
    }

    pub fn get_mut(&mut self, lift: Lift) -> &mut T {
        match lift {
            Lift::Squat => &mut self.squat,
            Lift::Bench => &mut self.bench,
            Lift::Deadlift => &mut self.deadlift,
        }
    }

    pub fn map<U>(&self, mut f: impl FnMut(Lift, &T) -> U) -> PerLift<U> {
        PerLift::from_fn(|lift| f(lift, self.get(lift)))
    }

    pub fn iter(&self) -> impl Iterator<Item = (Lift, &T)> {
        Lift::ALL.into_iter().map(|lift| (lift, self.get(lift)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iterates_in_meet_order() {
        let values = PerLift::from_fn(|lift| lift.to_string());
        let order: Vec<_> = values
            .iter()
            .map(|(lift, name)| (lift, name.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                (Lift::Squat, "squat"),
                (Lift::Bench, "bench"),
                (Lift::Deadlift, "deadlift")
            ]
        );
    }

    #[test]
    fn map_keeps_each_lift_in_place() {
        let doubled = PerLift {
            squat: 1,
            bench: 2,
            deadlift: 3,
        }
        .map(|_, v| v * 2);
        assert_eq!(
            doubled,
            PerLift {
                squat: 2,
                bench: 4,
                deadlift: 6
            }
        );
    }
}
