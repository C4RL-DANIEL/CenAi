//! Version comparison for the update manager.

use std::cmp::Ordering;

fn parse_version(value: &str) -> (Vec<i32>, Option<Vec<String>>) {
    let clean = value.strip_prefix('v').unwrap_or(value).split('+').next().unwrap_or("");
    let mut split = clean.splitn(2, '-');
    let core = split.next().unwrap_or("").split('.')
        .map(|part| part.parse::<i32>().unwrap_or(0)).collect();
    let prerelease = split.next().map(|part| part.split('.').map(str::to_owned).collect());
    (core, prerelease)
}

/// Compares app versions using the same ordering used by CenAI's release updater.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let (aa, ap) = parse_version(a);
    let (bb, bp) = parse_version(b);
    for index in 0..aa.len().max(bb.len()) {
        let left = *aa.get(index).unwrap_or(&0);
        let right = *bb.get(index).unwrap_or(&0);
        match left.cmp(&right) {
            Ordering::Less => return -1,
            Ordering::Greater => return 1,
            Ordering::Equal => {}
        }
    }
    let (ap, bp) = match (ap, bp) {
        (None, None) => return 0,
        (None, Some(_)) => return 1,
        (Some(_), None) => return -1,
        (Some(ap), Some(bp)) => (ap, bp),
    };
    for index in 0..ap.len().max(bp.len()) {
        let Some(left) = ap.get(index) else { return -1; };
        let Some(right) = bp.get(index) else { return 1; };
        let ln = left.parse::<i32>().ok();
        let rn = right.parse::<i32>().ok();
        let order = match (ln, rn) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => left.cmp(right),
        };
        match order {
            Ordering::Less => return -1,
            Ordering::Greater => return 1,
            Ordering::Equal => {}
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::compare_versions;

    #[test]
    fn compares_release_and_build_versions() {
        assert_eq!(compare_versions("v1.2.0+build.7", "1.1.9"), 1);
        assert_eq!(compare_versions("1.2.0+one", "1.2.0+two"), 0);
        assert_eq!(compare_versions("1.2", "1.2.0"), 0);
    }

    #[test]
    fn orders_prerelease_identifiers_before_stable_releases() {
        assert_eq!(compare_versions("1.0.0-beta.1", "1.0.0-beta.2"), -1);
        assert_eq!(compare_versions("1.0.0-beta", "1.0.0"), -1);
        assert_eq!(compare_versions("1.0.0-1", "1.0.0-alpha"), -1);
        assert_eq!(compare_versions("1.0.0-alpha", "1.0.0-1"), 1);
    }
}
