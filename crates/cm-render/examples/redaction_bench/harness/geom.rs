//! Rectangle arithmetic on `[x, y, w, h]` arrays (physical px).

pub type R = [f64; 4];

pub fn area(r: R) -> f64 {
    r[2].max(0.0) * r[3].max(0.0)
}

pub fn intersect(a: R, b: R) -> Option<R> {
    let x0 = a[0].max(b[0]);
    let y0 = a[1].max(b[1]);
    let x1 = (a[0] + a[2]).min(b[0] + b[2]);
    let y1 = (a[1] + a[3]).min(b[1] + b[3]);
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

pub fn expand(r: R, by: f64) -> R {
    [r[0] - by, r[1] - by, r[2] + 2.0 * by, r[3] + 2.0 * by]
}

/// Area of the union of `rects` (coordinate compression; the lists are tiny).
pub fn union_area(rects: &[R]) -> f64 {
    let rs: Vec<R> = rects.iter().copied().filter(|r| area(*r) > 0.0).collect();
    if rs.is_empty() {
        return 0.0;
    }
    let mut xs: Vec<f64> = rs.iter().flat_map(|r| [r[0], r[0] + r[2]]).collect();
    xs.sort_by(f64::total_cmp);
    xs.dedup();
    let mut total = 0.0;
    for w in xs.windows(2) {
        let (xa, xb) = (w[0], w[1]);
        let mut spans: Vec<(f64, f64)> = rs
            .iter()
            .filter(|r| r[0] <= xa && r[0] + r[2] >= xb)
            .map(|r| (r[1], r[1] + r[3]))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut covered = 0.0;
        let mut cur: Option<(f64, f64)> = None;
        for (s, e) in spans {
            match cur {
                Some((cs, ce)) if s <= ce => cur = Some((cs, ce.max(e))),
                Some((cs, ce)) => {
                    covered += ce - cs;
                    cur = Some((s, e));
                }
                None => cur = Some((s, e)),
            }
        }
        if let Some((cs, ce)) = cur {
            covered += ce - cs;
        }
        total += covered * (xb - xa);
    }
    total
}

/// Share (0..=1) of `target`'s area covered by the union of `by`.
pub fn coverage(target: R, by: &[R]) -> f64 {
    let a = area(target);
    if a <= 0.0 {
        return 0.0;
    }
    let clipped: Vec<R> = by.iter().filter_map(|r| intersect(target, *r)).collect();
    union_area(&clipped) / a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn union_of_overlapping_rects() {
        let u = union_area(&[[0.0, 0.0, 10.0, 10.0], [5.0, 0.0, 10.0, 10.0]]);
        assert!((u - 150.0).abs() < 1e-9);
    }

    #[test]
    fn coverage_counts_overlap_once() {
        let t = [0.0, 0.0, 100.0, 10.0];
        let c = coverage(t, &[[0.0, 0.0, 60.0, 10.0], [40.0, 0.0, 50.0, 10.0]]);
        assert!((c - 0.9).abs() < 1e-9);
    }
}
