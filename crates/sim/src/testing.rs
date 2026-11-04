use crate::rules::{Attempt, Weight};

pub fn kg(kg: f64) -> Weight {
    Weight::from_kg(kg).unwrap()
}

pub fn made(weight_kg: f64) -> Attempt {
    Attempt {
        weight: kg(weight_kg),
        made: true,
    }
}

pub fn missed(weight_kg: f64) -> Attempt {
    Attempt {
        made: false,
        ..made(weight_kg)
    }
}
