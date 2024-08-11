use std::collections::BinaryHeap;

use pizza_bot_rs_common::orders::{Distribution, Order, OrderAmount, PizzaAmount, PizzaKind, PizzaKindArray};

type SumAmount = usize;
type Penalty = f32;

fn calculate_cost(order: &Order, assigned: Distribution) -> Penalty {
    const epsilon: f32 = 0.0000001;

    let pref = 1.0 - order.preference;
    let count_pref = ((1.0 - pref) / pref) + 0.01;
    let shape_pref = (pref / (1.0 - pref)) + 0.01;

    let r_total: OrderAmount = order.amounts.sum();
    let a_total = assigned.sum();

    let total_diff = r_total.abs_diff(a_total);

    fn convert(diff: OrderAmount, total: OrderAmount, p: f32, more: bool) -> Penalty {
        let perc = diff as f32 / total as f32;
        if more {
            let mut scaled = 1.0 / (1.0 - (1.0 - 1.0 / (2.0 * total as f32)) * perc) - 1.0;
            if scaled < 0.0 {
                scaled = f32::INFINITY
            }
            return scaled * p
        } else {
            let scaled = perc / (1.0 - perc).max(0.0);
            return scaled * p
        }
    }

    let total_penalty = if total_diff == 0 {0.0} else {convert(total_diff, r_total, count_pref, a_total > r_total)};

    fn prepare_values(values: Distribution, total: OrderAmount) -> PizzaKindArray<f32> {
        values.map(|v| v as f32 / total as f32)
    }

    let r_perc = prepare_values(order.amounts, r_total);
    let a_perc = prepare_values(assigned, a_total);

    let diffs = r_perc.zip_map(a_perc, |r, a| if r > a {r - a} else {a - r});
    let scaled_diffs = diffs.map(|d| d * shape_pref);
    let pens = diffs.zip_map(scaled_diffs, |d, s| if d < epsilon {d} else {s});

    return total_penalty + 1.0 / (PizzaKind::Length as f32) * pens.sum::<Penalty>()
}

pub struct TotalPenalty {
    worst: f32,
    average: f32
}

impl TotalPenalty {
    fn add(&mut self, penalty: f32) {
        self.worst = self.worst.max(penalty);
        self.average += penalty
    }

    fn is_better_than(&self, that: &TotalPenalty) -> bool {
        let self_penalty = self.total();
        let that_penalty = that.total();

        if self_penalty < that_penalty {
            return true
        }
        if that_penalty < self_penalty {
            return false
        }

        if self.average < that.average {
            return true
        }
        if that.average < self.average {
            return false
        }

        return true
    }

    fn total(&self) -> f32 {
        const weight: f32 = 0.1;
        return (1.0 - weight) * self.worst + weight * self.average
    }
}

pub fn get_best(pieces_per_whole: OrderAmount, requests: &Vec<Order>) -> (TotalPenalty, PizzaKindArray<PizzaAmount>, Vec<Distribution>, bool) {
    let pieces_per_whole = pieces_per_whole as SumAmount;
    let mut totals: PizzaKindArray<SumAmount> = PizzaKindArray::splat(0);
    for req in requests {
        for (total, distr) in totals.iter_mut().zip(req.amounts) {
            *total += distr as SumAmount
        }
    }

    struct QueueElement {
        request_index: usize,
        offset: PizzaKindArray<bool>,
        penalty: f32
    }

    impl QueueElement {
        fn best_offset(adds: PizzaKindArray<bool>, deltas: PizzaKindArray<SumAmount>, request: &Order, assigned: Distribution, index: usize) -> Option<Self> {
            let mut best = None;
            let mut penalty = f32::INFINITY;
            'outer:
            for idx in 1..(1 << PizzaKind::Length) {
                let mut modify = PizzaKindArray::splat(false);
                for (i, (modify, delta)) in modify.iter_mut().zip(deltas).enumerate() {
                    *modify = (idx & (1 << i)) != 0;
                    if delta == 0 && *modify {
                        continue 'outer
                    }
                }

                let mut copy = assigned;
                for ((copy, modify), add) in copy.iter_mut().zip(modify).zip(adds) {
                    if modify {
                        if add {
                            *copy += 1
                        } else {
                            if *copy == 0 {continue 'outer}
                            *copy -= 1
                        }
                    }
                }

                let pen = calculate_cost(request, copy);
                if pen < penalty {
                    penalty = pen;
                    best = Some(modify)
                }
            }

            let Some(best) = best else {
                return None
            };

            return Some(Self {
                request_index: index,
                offset: best,
                penalty,
            })
        }
    }

    impl PartialEq for QueueElement {
        fn eq(&self, other: &Self) -> bool {
            self.penalty == other.penalty
        }
    }

    impl PartialOrd for QueueElement {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            other.penalty.partial_cmp(&self.penalty)
        }
    }

    impl Eq for QueueElement {}

    impl Ord for QueueElement {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            other.penalty.total_cmp(&self.penalty)
        }
    }

    let mut queue = BinaryHeap::new();
    let mut best_distribution = Vec::new();
    let mut best_config = PizzaKindArray::splat(false);
    let mut penalty = TotalPenalty {
        worst: f32::INFINITY,
        average: f32::INFINITY,
    };

    let mut next_distr = Vec::new();

    'outer:
    for idx in 0..(1 << PizzaKind::Length) {
        let mut adds = PizzaKindArray::splat(false);
        for (i, (add, total)) in adds.iter_mut().zip(totals).enumerate() {
            *add = (idx & (1 << i)) != 0;
            if total % pieces_per_whole == 0 && *add {
                continue 'outer
            }
        }

        let mut deltas = PizzaKindArray::splat(0);
        for ((delta, add), total) in deltas.iter_mut().zip(adds).zip(totals) {
            let mut target = (total / pieces_per_whole) * pieces_per_whole;
            if add {
                target += pieces_per_whole;
                *delta = target - total
            } else {
                *delta = total - target
            }
        }

        queue.clear();
        next_distr.clear();
        next_distr.reserve_exact(requests.len());
        for (i, req) in requests.iter().enumerate() {
            next_distr.push(req.amounts);
            if let Some(best) = QueueElement::best_offset(adds, deltas, req, req.amounts, i) {
                queue.push(best)
            }
        }

        let mut pen = TotalPenalty {
            worst: 0.0,
            average: 0.0,
        };

        while deltas.sum::<SumAmount>() != 0 {
            let Some(element) = queue.pop() else {
                continue 'outer
            };

            'blk: {
                for (offset, delta) in element.offset.into_iter().zip(deltas) {
                    if offset && delta == 0 {break 'blk}
                }

                for (((next, delta), offset), add) in next_distr[element.request_index].iter_mut().zip(&mut deltas).zip(element.offset).zip(adds) {
                    if offset {
                        if add {
                            *next += 1
                        } else {
                            *next -= 1
                        }
                        *delta -= 1;
                        pen.add(element.penalty)
                    }
                }
            }

            if let Some(best) = QueueElement::best_offset(adds, deltas, &requests[element.request_index], next_distr[element.request_index], element.request_index) {
                queue.push(best)
            }
        }

        if pen.is_better_than(&penalty) {
            penalty = pen;
            best_config = adds;
            (next_distr, best_distribution) = (best_distribution, next_distr);
        }
    }

    let mut config = best_config.zip_map(totals, |c, t| {
        let mut target = t / pieces_per_whole;
        if c {
            target += 1
        }
        target as PizzaAmount
    });

    if best_distribution.len() != requests.len() {
        debug_assert!(next_distr.len() == requests.len());
        best_distribution = next_distr;
    }

    let is_valid = !penalty.worst.is_infinite();

    if !is_valid {
        for distr in &mut best_distribution {
            *distr = PizzaKindArray::splat(0)
        }

        config = PizzaKindArray::splat(0)
    }

    return (penalty, config, best_distribution, is_valid)
}
