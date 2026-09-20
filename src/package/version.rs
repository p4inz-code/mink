//! Package versions and version requirements.
//!
//! Versions are `MAJOR.MINOR.PATCH` with an optional `-prerelease` tail
//! (the manifest format in `docs/ecosystem/PACKAGE_ARCHITECTURE.md` §2).
//! Requirements follow the same document's §4 table:
//!
//! | Spelling | Meaning |
//! |---|---|
//! | `1.2.3` (bare) | compatible — `>=1.2.3, <2.0.0` (the documented default) |
//! | `^1.2.3` | compatible — `>=1.2.3, <2.0.0` |
//! | `~1.2.3` | patch-level — `>=1.2.3, <1.3.0` |
//! | `=1.2.3` | exactly `1.2.3` |
//! | `>=1.0, <2.0` | an explicit comma-separated comparator set |
//! | `*` | any version |
//!
//! A requirement is a conjunction: every comma-separated comparator must
//! hold, and all of them must match the same version.

use std::cmp::Ordering;
use std::fmt;

/// A parsed semantic version.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Version {
    /// Major number: breaking changes.
    pub major: u64,
    /// Minor number: backwards-compatible additions.
    pub minor: u64,
    /// Patch number: backwards-compatible fixes.
    pub patch: u64,
    /// The `-prerelease` tail, without the leading `-`.
    pub prerelease: Option<String>,
}

impl Version {
    /// A `major.minor.patch` release version.
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Self {
            major,
            minor,
            patch,
            prerelease: None,
        }
    }

    /// Parses `MAJOR.MINOR.PATCH[-prerelease]`.
    ///
    /// The numeric components are required; MINK does not accept the
    /// `1`/`1.2` shorthands (the manifest always spells all three).
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(VersionError::Empty);
        }
        let (numbers, prerelease) = match text.split_once('-') {
            Some((numbers, tail)) => {
                if tail.is_empty() {
                    return Err(VersionError::EmptyPrerelease(text.to_string()));
                }
                if !tail
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
                {
                    return Err(VersionError::BadPrerelease(text.to_string()));
                }
                (numbers, Some(tail.to_string()))
            }
            None => (text, None),
        };
        let parts: Vec<&str> = numbers.split('.').collect();
        if parts.len() != 3 {
            return Err(VersionError::NotThreeParts(text.to_string()));
        }
        let mut numbers = [0u64; 3];
        for (slot, part) in numbers.iter_mut().zip(parts) {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(VersionError::BadNumber(text.to_string()));
            }
            *slot = part
                .parse()
                .map_err(|_| VersionError::BadNumber(text.to_string()))?;
        }
        Ok(Self {
            major: numbers[0],
            minor: numbers[1],
            patch: numbers[2],
            prerelease,
        })
    }

    /// Whether this version can satisfy `requirement` (see [`Requirement`]).
    pub fn matches(&self, requirement: &Requirement) -> bool {
        requirement.matches(self)
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        // Release versions order above their prereleases: `1.0.0` > `1.0.0-rc.1`.
        let core =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if core != Ordering::Equal {
            return core;
        }
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if let Some(pre) = &self.prerelease {
            write!(f, "-{pre}")?;
        }
        Ok(())
    }
}

/// A version requirement: one or more comparators that must all hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    /// The original spelling, for diagnostics.
    pub text: String,
    /// The comparators, in the order they were written.
    pub comparators: Vec<Comparator>,
}

/// One comparator inside a requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Comparator {
    /// `=1.2.3`: exactly this version.
    Exactly(Version),
    /// `^1.2.3`: `>=1.2.3` and the same leftmost non-zero component.
    Compatible(Version),
    /// `~1.2.3`: `>=1.2.3` and the same major/minor.
    PatchLevel(Version),
    /// `>=1.2.3`.
    AtLeast(Version),
    /// `>1.2.3`.
    Above(Version),
    /// `<=1.2.3`.
    AtMost(Version),
    /// `<1.2.3`.
    Below(Version),
}

impl Comparator {
    /// Whether `version` satisfies this comparator.
    pub fn matches(&self, version: &Version) -> bool {
        match self {
            Self::Exactly(want) => version == want,
            Self::Compatible(min) => {
                if version < min {
                    return false;
                }
                // `^0.2.3` is `>=0.2.3, <0.3.0`; `^1.2.3` is `>=1.2.3, <2.0.0`.
                if min.major != 0 {
                    version.major == min.major
                } else if min.minor != 0 {
                    version.major == 0 && version.minor == min.minor
                } else {
                    version.major == 0 && version.minor == 0 && version.patch == min.patch
                }
            }
            Self::PatchLevel(min) => {
                if version < min {
                    return false;
                }
                version.major == min.major && version.minor == min.minor
            }
            Self::AtLeast(min) => version >= min,
            Self::Above(min) => version > min,
            Self::AtMost(max) => version <= max,
            Self::Below(max) => version < max,
        }
    }
}

impl Requirement {
    /// A requirement that accepts every version (`*`).
    pub fn any() -> Self {
        Self {
            text: "*".to_string(),
            comparators: Vec::new(),
        }
    }

    /// Whether this is the `*` requirement.
    pub fn is_any(&self) -> bool {
        self.comparators.is_empty()
    }

    /// Parses a requirement string.
    ///
    /// Comparators are separated by commas and/or whitespace; a bare version
    /// means the documented compatible default (`^`).
    pub fn parse(text: &str) -> Result<Self, VersionError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(VersionError::Empty);
        }
        if trimmed == "*" {
            return Ok(Self::any());
        }
        let mut comparators = Vec::new();
        for part in trimmed.split([',', ' ']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            comparators.push(parse_comparator(part, trimmed)?);
        }
        if comparators.is_empty() {
            return Err(VersionError::Empty);
        }
        Ok(Self {
            text: trimmed.to_string(),
            comparators,
        })
    }

    /// Whether `version` satisfies every comparator.
    pub fn matches(&self, version: &Version) -> bool {
        self.comparators
            .iter()
            .all(|comparator| comparator.matches(version))
    }

    /// Whether any version can satisfy this requirement.
    ///
    /// Every comparator is read as an interval and the intervals must
    /// intersect; `>=2.0, <1.0` and `=1.0.0, ^2.0.0` are therefore rejected
    /// up front instead of failing only because no candidate happens to
    /// exist. The resolver still checks the real candidate set, so a
    /// requirement that survives this check can fail to resolve.
    pub fn is_satisfiable(&self) -> bool {
        for (index, left) in self.comparators.iter().enumerate() {
            for right in &self.comparators[index + 1..] {
                if !comparators_overlap(left, right) {
                    return false;
                }
            }
        }
        true
    }
}

/// A version bound: the version and whether it is inclusive.
type Bound = (Version, bool);

/// Whether two comparators can be satisfied by some single version.
fn comparators_overlap(left: &Comparator, right: &Comparator) -> bool {
    let (low_left, high_left) = interval(left);
    let (low_right, high_right) = interval(right);
    let low = match (low_left, low_right) {
        (None, other) | (other, None) => other,
        (Some(a), Some(b)) => Some(if a.0 > b.0 {
            a
        } else if b.0 > a.0 {
            b
        } else {
            (a.0, a.1 && b.1)
        }),
    };
    let high = match (high_left, high_right) {
        (None, other) | (other, None) => other,
        (Some(a), Some(b)) => Some(if a.0 < b.0 {
            a
        } else if b.0 < a.0 {
            b
        } else {
            (a.0, a.1 && b.1)
        }),
    };
    match (low, high) {
        (Some((low, low_inclusive)), Some((high, high_inclusive))) => {
            low < high || (low == high && low_inclusive && high_inclusive)
        }
        _ => true,
    }
}

/// The `[low, high]` interval a comparator accepts, with `None` for an
/// unbounded side.
fn interval(comparator: &Comparator) -> (Option<Bound>, Option<Bound>) {
    match comparator {
        Comparator::Exactly(v) => (Some((v.clone(), true)), Some((v.clone(), true))),
        Comparator::AtLeast(v) => (Some((v.clone(), true)), None),
        Comparator::Above(v) => (Some((v.clone(), false)), None),
        Comparator::AtMost(v) => (None, Some((v.clone(), true))),
        Comparator::Below(v) => (None, Some((v.clone(), false))),
        Comparator::Compatible(min) => (Some((min.clone(), true)), Some(compatible_max(min))),
        Comparator::PatchLevel(min) => (
            Some((min.clone(), true)),
            Some((Version::new(min.major, min.minor, u64::MAX), true)),
        ),
    }
}

/// The inclusive maximum version a `^` requirement accepts: everything below
/// the next release of the leftmost non-zero component.
fn compatible_max(min: &Version) -> Bound {
    if min.major != 0 {
        (Version::new(min.major, u64::MAX, u64::MAX), true)
    } else if min.minor != 0 {
        (Version::new(0, min.minor, u64::MAX), true)
    } else {
        (Version::new(0, 0, min.patch), true)
    }
}

impl fmt::Display for Requirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// A malformed version or requirement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionError {
    /// The text was empty.
    Empty,
    /// A version component was missing: three are required.
    NotThreeParts(String),
    /// A version component was not a number.
    BadNumber(String),
    /// A requirement operator was not recognised.
    UnknownOperator(String),
    /// A prerelease tail was empty.
    EmptyPrerelease(String),
    /// A prerelease tail contained an illegal character.
    BadPrerelease(String),
}

impl fmt::Display for VersionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "empty version or requirement"),
            Self::NotThreeParts(text) => {
                write!(f, "version '{text}' must have exactly three components")
            }
            Self::BadNumber(text) => {
                write!(f, "version '{text}' has a non-numeric component")
            }
            Self::UnknownOperator(text) => {
                write!(f, "unknown version requirement operator in '{text}'")
            }
            Self::EmptyPrerelease(text) => {
                write!(f, "version '{text}' ends with an empty prerelease")
            }
            Self::BadPrerelease(text) => {
                write!(f, "version '{text}' has an illegal prerelease")
            }
        }
    }
}

impl std::error::Error for VersionError {}

/// Parses one comparator such as `^1.2.3` or `>=1.0`.
fn parse_comparator(part: &str, whole: &str) -> Result<Comparator, VersionError> {
    let (operator, rest) = split_operator(part);
    // `>=1.0` and `1.0.2` are written without a third component in the
    // architecture doc's range example, so a two-component bound means a
    // zero patch.
    let version = parse_bound(rest, whole)?;
    Ok(match operator {
        "=" => Comparator::Exactly(version),
        "^" => Comparator::Compatible(version),
        "~" => Comparator::PatchLevel(version),
        ">=" => Comparator::AtLeast(version),
        ">" => Comparator::Above(version),
        "<=" => Comparator::AtMost(version),
        "<" => Comparator::Below(version),
        "" => Comparator::Compatible(version),
        _ => return Err(VersionError::UnknownOperator(whole.to_string())),
    })
}

/// Splits a leading comparison operator off a comparator.
fn split_operator(part: &str) -> (&str, &str) {
    for operator in [">=", "<=", "^", "~", ">", "<", "="] {
        if let Some(rest) = part.strip_prefix(operator) {
            return (operator, rest.trim());
        }
    }
    ("", part)
}

/// Parses a version bound, accepting the two-component range spelling.
fn parse_bound(text: &str, whole: &str) -> Result<Version, VersionError> {
    match Version::parse(text) {
        Ok(version) => Ok(version),
        Err(error) => {
            let parts: Vec<&str> = text.split('.').collect();
            if parts.len() == 2 && parts.iter().all(|p| !p.is_empty()) {
                if let Ok(version) = Version::parse(&format!("{text}.0")) {
                    return Ok(version);
                }
            }
            Err(match error {
                VersionError::Empty | VersionError::NotThreeParts(_) => {
                    VersionError::NotThreeParts(whole.to_string())
                }
                other => other,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        Version::parse(text).expect("parses")
    }

    #[test]
    fn versions_parse_and_display_round_trip() {
        for text in ["1.0.0", "0.0.1", "10.20.30", "1.2.3-rc.1", "1.0.0-alpha"] {
            assert_eq!(v(text).to_string(), text);
        }
    }

    #[test]
    fn malformed_versions_are_rejected() {
        for text in ["", "1", "1.2", "1.2.3.4", "a.b.c", "1.2.x", "1.2.3-"] {
            assert!(Version::parse(text).is_err(), "{text:?} should be rejected");
        }
    }

    #[test]
    fn ordering_is_numeric_then_prerelease_aware() {
        assert!(v("1.0.0") < v("1.0.1"));
        assert!(v("1.0.0") < v("1.1.0"));
        assert!(v("1.0.0") < v("2.0.0"));
        assert!(v("9.9.9") < v("10.0.0"));
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        assert!(v("1.0.0-alpha") < v("1.0.0-beta"));
    }

    #[test]
    fn bare_version_is_the_compatible_default() {
        let requirement = Requirement::parse("1.2.3").expect("parses");
        assert!(requirement.matches(&v("1.2.3")));
        assert!(requirement.matches(&v("1.9.0")));
        assert!(!requirement.matches(&v("2.0.0")));
        assert!(!requirement.matches(&v("1.2.2")));
    }

    #[test]
    fn caret_requirements_follow_the_leftmost_nonzero_component() {
        let zero_major = Requirement::parse("^0.2.3").expect("parses");
        assert!(zero_major.matches(&v("0.2.3")));
        assert!(zero_major.matches(&v("0.2.9")));
        assert!(!zero_major.matches(&v("0.3.0")));
        let zero_minor = Requirement::parse("^0.0.3").expect("parses");
        assert!(zero_minor.matches(&v("0.0.3")));
        assert!(!zero_minor.matches(&v("0.0.4")));
        let major = Requirement::parse("^1.2.3").expect("parses");
        assert!(major.matches(&v("1.99.0")));
        assert!(!major.matches(&v("2.0.0")));
    }

    #[test]
    fn tilde_and_exact_requirements() {
        let tilde = Requirement::parse("~1.2.3").expect("parses");
        assert!(tilde.matches(&v("1.2.3")));
        assert!(tilde.matches(&v("1.2.99")));
        assert!(!tilde.matches(&v("1.3.0")));
        let exact = Requirement::parse("=1.2.3").expect("parses");
        assert!(exact.matches(&v("1.2.3")));
        assert!(!exact.matches(&v("1.2.4")));
    }

    #[test]
    fn range_requirements_are_conjunctions() {
        let range = Requirement::parse(">=1.0, <2.0").expect("parses");
        assert!(range.matches(&v("1.0.0")));
        assert!(range.matches(&v("1.9.9")));
        assert!(!range.matches(&v("2.0.0")));
        assert!(!range.matches(&v("0.9.9")));
        // Two-component bounds mean a zero patch.
        assert!(range.matches(&v("1.5.0")));
    }

    #[test]
    fn any_requirement_accepts_everything() {
        let any = Requirement::parse("*").expect("parses");
        assert!(any.is_any());
        assert!(any.matches(&v("0.0.1")));
        assert!(any.matches(&v("99.99.99")));
    }

    #[test]
    fn impossible_requirements_are_detected() {
        assert!(
            !Requirement::parse(">=2.0, <1.0")
                .expect("parses")
                .is_satisfiable()
        );
        assert!(
            !Requirement::parse("=1.0.0, ^2.0.0")
                .expect("parses")
                .is_satisfiable()
        );
        assert!(
            Requirement::parse(">=1.0, <2.0")
                .expect("parses")
                .is_satisfiable()
        );
        assert!(
            Requirement::parse("^1.2, ~1.2.3")
                .expect("parses")
                .is_satisfiable()
        );
    }

    #[test]
    fn two_component_bounds_are_used_by_ranges_and_bare_requirements() {
        // The architecture doc spells range bounds as `>=1.0`, so a missing
        // component means a zero patch rather than a syntax error.
        let bare = Requirement::parse("1.2").expect("parses");
        assert_eq!(
            bare.comparators,
            Requirement::parse("^1.2.0").expect("parses").comparators
        );
        assert!(bare.matches(&v("1.9.0")));
        assert!(!bare.matches(&v("2.0.0")));
        let range = Requirement::parse(">=1.2").expect("parses");
        assert!(range.matches(&v("1.2.0")));
        assert!(!range.matches(&v("1.1.9")));
    }

    #[test]
    fn malformed_requirements_are_rejected() {
        for text in ["", "^", ">=", "^1.2.3.4", "??1.0.0", "1.2.3.4"] {
            assert!(
                Requirement::parse(text).is_err(),
                "{text:?} should be rejected"
            );
        }
    }
}
