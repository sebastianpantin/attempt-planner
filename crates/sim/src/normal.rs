//! Averaging over a standard normal draw without sampling it.

use std::sync::LazyLock;

const DRAWS: usize = 121;
const DRAW_RANGE: f64 = 6.0;

static DRAW_TABLE: LazyLock<Vec<(f64, f64)>> = LazyLock::new(|| normal_grid(DRAWS, DRAW_RANGE));

/// `count` evenly spaced points from `-range` to `range` with weights that average a
/// function over a standard normal draw.
pub fn normal_grid(count: usize, range: f64) -> Vec<(f64, f64)> {
    let width = 2.0 * range / (count - 1) as f64;
    let points = (0..count).map(|i| -range + i as f64 * width);
    let density = |z: f64| (-z * z / 2.0).exp();
    let total: f64 = points.clone().map(density).sum();
    points.map(|z| (z, density(z) / total)).collect()
}

/// Evenly spaced points from −6 to 6 with weights that average a function over a standard
/// normal draw, worked out once.
pub fn normal_draws() -> impl Iterator<Item = (f64, f64)> + Clone {
    DRAW_TABLE.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_give_the_moments_of_a_standard_normal() {
        let moment = |power: i32| normal_draws().map(|(z, w)| w * z.powi(power)).sum::<f64>();
        assert!((moment(0) - 1.0).abs() < 1e-12);
        assert!(moment(1).abs() < 1e-12);
        assert!((moment(2) - 1.0).abs() < 1e-6);
    }
}
