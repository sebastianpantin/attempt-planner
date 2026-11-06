//! Spreads work over every core.

use std::num::NonZeroUsize;
use std::ops::AddAssign;
use std::sync::atomic::{AtomicUsize, Ordering};

fn threads() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Folds `work` over `items` on every core, each thread taking the next item when free,
/// for items that take very different times.
pub fn in_parallel<T: Sync, R: Default + Send + for<'a> AddAssign<&'a R>>(
    items: &[T],
    work: impl Fn(&T) -> R + Sync,
) -> R {
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads())
            .map(|_| {
                scope.spawn(|| {
                    let mut total = R::default();
                    while let Some(item) = items.get(next.fetch_add(1, Ordering::Relaxed)) {
                        total += &work(item);
                    }
                    total
                })
            })
            .collect();
        workers.into_iter().fold(R::default(), |mut total, worker| {
            total += &worker.join().expect("a worker thread panicked");
            total
        })
    })
}

/// The results of `work` on `items` that are `Some`, in order, worked out on every core
/// like [`in_parallel`].
pub fn filter_map_in_parallel<T: Sync, R: Send>(
    items: &[T],
    work: impl Fn(&T) -> Option<R> + Sync,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let mut found: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..threads())
            .map(|_| {
                scope.spawn(|| {
                    let mut found = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            return found;
                        };
                        found.extend(work(item).map(|result| (index, result)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("a worker thread panicked"))
            .collect()
    });
    found.sort_by_key(|(index, _)| *index);
    found.into_iter().map(|(_, result)| result).collect()
}

/// `work` on each of a few items, each on a thread of its own, in order.
pub fn map_in_parallel<T: Sync, R: Send>(items: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    std::thread::scope(|scope| {
        let work = &work;
        let workers: Vec<_> = items
            .iter()
            .map(|item| scope.spawn(move || work(item)))
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("a worker thread panicked"))
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_up_the_work_on_every_item() {
        let items: Vec<u64> = (1..=1_000).collect();
        #[derive(Default)]
        struct Sum(u64);
        impl AddAssign<&Sum> for Sum {
            fn add_assign(&mut self, other: &Sum) {
                self.0 += other.0;
            }
        }
        assert_eq!(in_parallel(&items, |&item| Sum(item)).0, 500_500);
    }

    #[test]
    fn keeps_the_results_that_are_some_in_order() {
        let items: Vec<u64> = (1..=1_000).collect();
        let even = filter_map_in_parallel(&items, |&item| (item % 2 == 0).then_some(item));
        assert_eq!(even, (1..=500).map(|i| i * 2).collect::<Vec<_>>());
    }

    #[test]
    fn maps_each_item_in_order() {
        assert_eq!(map_in_parallel(&[1, 2, 3], |item| item * 10), [10, 20, 30]);
    }
}
