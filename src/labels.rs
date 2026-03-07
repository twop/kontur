/// Iterator that produces selection labels: single-character labels first,
/// then two-character labels.
///
/// # Character pools
///
/// - `single`: chars used as 1-char labels. These chars are **never** used as
///   the first character of a 2-char label, so a 1-char label can never be a
///   prefix of a 2-char label.
/// - `double`: chars used as the first character of 2-char labels.
///
/// The second character of every 2-char label is drawn from `single ∪ double`
/// (i.e. all chars from both slices, `single` first).
///
/// # Example
///
/// ```
/// use kontur::labels::LabelIter;
///
/// let labels: Vec<String> = LabelIter::new(&['a', 'b'], &['x'])
///     .collect();
/// assert_eq!(labels, ["a", "b", "xa", "xb", "xx"]);
/// ```
pub struct LabelIter {
    single: &'static [char],
    double: &'static [char],
    phase: Phase,
}

/// Internal state of the iterator.
#[derive(Debug, Clone, Copy)]
enum Phase {
    /// Yielding single-character labels. `i` is the next index into `single`.
    Single(usize),
    /// Yielding two-character labels.
    /// `i` indexes into `double` (first char).
    /// `j` indexes into `single ∪ double` (second char):
    //  -  j < single.len()           → single[`j`]
    //  -  j >= single.len()          → double[`j` - single.len()]
    Double(usize, usize),
    /// Exhausted.
    Done,
}

impl LabelIter {
    pub fn new(single: &'static [char], double: &'static [char]) -> Self {
        let phase = if !single.is_empty() {
            Phase::Single(0)
        } else if !double.is_empty() {
            Phase::Double(0, 0)
        } else {
            Phase::Done
        };
        Self {
            single,
            double,
            phase,
        }
    }

    /// Total length of the `single ∪ double` pool (used for second char).
    #[inline]
    fn all_len(&self) -> usize {
        self.single.len() + self.double.len()
    }

    /// Index into the combined `single ∪ double` pool.
    #[inline]
    fn all_char(&self, j: usize) -> char {
        if j < self.single.len() {
            self.single[j]
        } else {
            self.double[j - self.single.len()]
        }
    }
}

impl Iterator for LabelIter {
    type Item = String;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.phase {
                Phase::Single(i) => {
                    if i < self.single.len() {
                        self.phase = Phase::Single(i + 1);
                        return Some(self.single[i].to_string());
                    } else {
                        // Transition to double phase.
                        self.phase = if !self.double.is_empty() && self.all_len() > 0 {
                            Phase::Double(0, 0)
                        } else {
                            Phase::Done
                        };
                    }
                }
                Phase::Double(i, j) => {
                    if i >= self.double.len() {
                        self.phase = Phase::Done;
                        continue;
                    }
                    if j >= self.all_len() {
                        // Advance first char, reset second.
                        self.phase = Phase::Double(i + 1, 0);
                        continue;
                    }
                    let label = format!("{}{}", self.double[i], self.all_char(j));
                    self.phase = Phase::Double(i, j + 1);
                    return Some(label);
                }
                Phase::Done => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(single: &'static [char], double: &'static [char]) -> Vec<String> {
        LabelIter::new(single, double).collect()
    }

    fn strs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// One double-prefix char, two single chars.
    /// second-char pool = ['a', 'b', 'x']
    #[test]
    fn one_double_two_single() {
        assert_eq!(
            collect(&['a', 'b'], &['x']),
            strs(&["a", "b", "xa", "xb", "xx"]),
        );
    }

    /// Two double-prefix chars, one single char.
    /// second-char pool = ['a', 'x', 'y']
    #[test]
    fn two_double_one_single() {
        assert_eq!(
            collect(&['a'], &['x', 'y']),
            strs(&["a", "xa", "xx", "xy", "ya", "yx", "yy"]),
        );
    }

    /// No single chars: only 2-char labels are produced.
    #[test]
    fn empty_single() {
        assert_eq!(collect(&[], &['x', 'y']), strs(&["xx", "xy", "yx", "yy"]),);
    }

    /// No double chars: only 1-char labels are produced.
    #[test]
    fn empty_double() {
        assert_eq!(collect(&['a', 'b'], &[]), strs(&["a", "b"]),);
    }

    /// Both empty: iterator terminates immediately.
    #[test]
    fn both_empty() {
        assert_eq!(collect(&[], &[]), strs(&[]));
    }

    /// Single chars come before any double-char labels.
    #[test]
    fn ordering_single_before_double() {
        let labels = collect(&['a', 'b'], &['x', 'y']);
        let first_double = labels.iter().position(|l| l.len() == 2).unwrap();
        assert!(
            labels[..first_double].iter().all(|l| l.len() == 1),
            "all single-char labels must precede all double-char labels"
        );
    }

    /// No 2-char label may start with a char that appears in `single`.
    #[test]
    fn no_single_prefix_in_double_labels() {
        let single: &[char] = &['a', 'b'];
        let double: &[char] = &['x', 'y'];
        let labels = collect(single, double);
        for label in labels.iter().filter(|l| l.len() == 2) {
            let first = label.chars().next().unwrap();
            assert!(
                !single.contains(&first),
                "double-char label {:?} starts with a single-char label char",
                label
            );
        }
    }

    /// All produced labels are unique.
    #[test]
    fn all_unique() {
        let labels = collect(&['a', 'b'], &['x', 'y']);
        let mut seen = std::collections::HashSet::new();
        for label in &labels {
            assert!(seen.insert(label.clone()), "duplicate label: {:?}", label);
        }
    }
}
