//! Nelder-Mead: finds a low point of a function without needing its gradient.

/// The point near `start` where `f` is lowest, exploring first by about `steps`.
pub fn minimize<const N: usize>(
    f: impl Fn([f64; N]) -> f64,
    start: [f64; N],
    steps: [f64; N],
) -> [f64; N] {
    minimize_to(f, start, steps, 1e-14, 5_000)
}

/// Like [`minimize`], stopping once the values of the search's points are within
/// `tolerance` of each other, relative to the lowest, or after `max_evaluations` calls of
/// `f`: for a costly `f`.
pub fn minimize_to<const N: usize>(
    f: impl Fn([f64; N]) -> f64,
    start: [f64; N],
    steps: [f64; N],
    tolerance: f64,
    max_evaluations: usize,
) -> [f64; N] {
    let mut simplex: Vec<([f64; N], f64)> = std::iter::once(start)
        .chain((0..N).map(|i| {
            let mut point = start;
            point[i] += steps[i];
            point
        }))
        .map(|point| (point, f(point)))
        .collect();
    let towards = |from: [f64; N], to: [f64; N], t: f64| -> [f64; N] {
        std::array::from_fn(|i| from[i] + t * (to[i] - from[i]))
    };
    let mut evaluations = simplex.len();
    while evaluations < max_evaluations {
        simplex.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (best, worst) = (simplex[0].1, simplex[N].1);
        if worst - best <= tolerance * (best.abs() + tolerance) {
            break;
        }
        let centre: [f64; N] =
            std::array::from_fn(|i| simplex[..N].iter().map(|(p, _)| p[i]).sum::<f64>() / N as f64);
        let worst_point = simplex[N].0;
        let reflected = towards(worst_point, centre, 2.0);
        let reflected_value = f(reflected);
        evaluations += 1;
        if reflected_value < best {
            let expanded = towards(worst_point, centre, 3.0);
            let expanded_value = f(expanded);
            evaluations += 1;
            simplex[N] = if expanded_value < reflected_value {
                (expanded, expanded_value)
            } else {
                (reflected, reflected_value)
            };
        } else if reflected_value < simplex[N - 1].1 {
            simplex[N] = (reflected, reflected_value);
        } else {
            let contracted = towards(worst_point, centre, 0.5);
            let contracted_value = f(contracted);
            evaluations += 1;
            if contracted_value < simplex[N].1 {
                simplex[N] = (contracted, contracted_value);
            } else {
                let best_point = simplex[0].0;
                for vertex in &mut simplex[1..] {
                    let point = towards(best_point, vertex.0, 0.5);
                    *vertex = (point, f(point));
                }
                evaluations += N;
            }
        }
    }
    simplex
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(point, _)| point)
        .unwrap_or(start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_bottom_of_a_bowl() {
        let found = minimize(
            |[x, y]| (x - 1.5).powi(2) + 3.0 * (y + 0.5).powi(2) + 2.0,
            [0.0, 0.0],
            [0.5, 0.5],
        );
        assert!(
            (found[0] - 1.5).abs() < 1e-5 && (found[1] + 0.5).abs() < 1e-5,
            "{found:?}"
        );
    }

    #[test]
    fn follows_a_curved_valley() {
        let found = minimize(
            |[x, y]| (1.0 - x).powi(2) + 100.0 * (y - x * x).powi(2),
            [-1.2, 1.0],
            [0.1, 0.1],
        );
        assert!(
            (found[0] - 1.0).abs() < 1e-3 && (found[1] - 1.0).abs() < 1e-3,
            "{found:?}"
        );
    }
}
