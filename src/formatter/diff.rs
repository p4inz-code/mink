//! A small line diff, for `mink fmt --diff`.

/// One line of an edit script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Equal(usize, usize),
    Delete(usize),
    Insert(usize),
}

/// The edit script that turns `old` into `new`, by longest common subsequence.
/// Returns `None` when the files are too large for the quadratic table.
fn script(old: &[&str], new: &[&str]) -> Option<Vec<Op>> {
    let (n, m) = (old.len(), new.len());
    if n.checked_mul(m)? > 16_000_000 {
        return None;
    }
    let width = m + 1;
    let mut table = vec![0u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if old[i] == new[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut ops = Vec::new();
    while i < n && j < m {
        if old[i] == new[j] {
            ops.push(Op::Equal(i, j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j] >= table[i * width + j + 1] {
            ops.push(Op::Delete(i));
            i += 1;
        } else {
            ops.push(Op::Insert(j));
            j += 1;
        }
    }
    ops.extend((i..n).map(Op::Delete));
    ops.extend((j..m).map(Op::Insert));
    Some(ops)
}

/// A unified diff of `old` against `new` with three lines of context, or an empty string when equal.
pub fn unified_diff(name: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    let mut out = format!("--- {name}\n+++ {name} (formatted)\n");
    let Some(ops) = script(&old_lines, &new_lines) else {
        out.push_str("(the file is too large to show a line diff; run mink fmt to reformat it)\n");
        return out;
    };

    const CONTEXT: usize = 3;
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, op)| !matches!(op, Op::Equal(..)))
        .map(|(index, _)| index)
        .collect();
    let mut at = 0;
    while at < changed.len() {
        // A hunk runs from three lines before its first change to three lines after its last,
        // merging changes that are close enough to share context.
        let mut last = at;
        while last + 1 < changed.len() && changed[last + 1] - changed[last] <= 2 * CONTEXT + 1 {
            last += 1;
        }
        let start = changed[at].saturating_sub(CONTEXT);
        let end = (changed[last] + CONTEXT + 1).min(ops.len());
        let hunk = &ops[start..end];
        let old_start = hunk
            .iter()
            .find_map(|op| match op {
                Op::Equal(i, _) | Op::Delete(i) => Some(*i + 1),
                Op::Insert(_) => None,
            })
            .unwrap_or(1);
        let new_start = hunk
            .iter()
            .find_map(|op| match op {
                Op::Equal(_, j) | Op::Insert(j) => Some(*j + 1),
                Op::Delete(_) => None,
            })
            .unwrap_or(1);
        let old_count = hunk.iter().filter(|op| !matches!(op, Op::Insert(_))).count();
        let new_count = hunk.iter().filter(|op| !matches!(op, Op::Delete(_))).count();
        out.push_str(&format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n"));
        for op in hunk {
            match op {
                Op::Equal(i, _) => out.push_str(&format!(" {}\n", old_lines[*i])),
                Op::Delete(i) => out.push_str(&format!("-{}\n", old_lines[*i])),
                Op::Insert(j) => out.push_str(&format!("+{}\n", new_lines[*j])),
            }
        }
        at = last + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_text_has_no_diff() {
        assert_eq!(unified_diff("a.mink", "x\ny\n", "x\ny\n"), "");
    }

    #[test]
    fn a_changed_line_is_shown_with_context() {
        let old = "a\nb\nc\nd\ne\nf\ng\n";
        let new = "a\nb\nc\nD\ne\nf\ng\n";
        let diff = unified_diff("f.mink", old, new);
        assert!(diff.starts_with("--- f.mink\n+++ f.mink (formatted)\n@@ -1,7 +1,7 @@\n"), "{diff}");
        assert!(diff.contains("-d\n+D\n"), "{diff}");
        assert!(diff.contains(" c\n"), "{diff}");
    }

    #[test]
    fn distant_changes_become_separate_hunks() {
        let old: String = (1..=30).map(|n| format!("line{n}\n")).collect();
        let new = old.replace("line2\n", "LINE2\n").replace("line29\n", "LINE29\n");
        let diff = unified_diff("f.mink", &old, &new);
        assert_eq!(diff.matches("@@ ").count(), 2, "{diff}");
    }

    #[test]
    fn insertions_and_deletions() {
        let diff = unified_diff("f.mink", "a\nb\n", "a\nx\nb\n");
        assert!(diff.contains("+x\n"), "{diff}");
        let diff = unified_diff("f.mink", "a\nx\nb\n", "a\nb\n");
        assert!(diff.contains("-x\n"), "{diff}");
    }
}
