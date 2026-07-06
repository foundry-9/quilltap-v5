//! Shared line-level diff primitive — v4 `lib/doc-edit/line-diff.ts`.
//!
//! A Myers O(ND) shortest-edit-script diff over arrays of lines. Used both by
//! the unified-diff generator ([`crate::doc_edit::unified_diff`]) and by the
//! in-editor change gutter, so a change is recognized the same way everywhere:
//! matched entries stay put, only genuinely different entries are reported as
//! removed/inserted, and content that merely shifted position is not treated as
//! churn.
//!
//! This is a **transcription** of v4's Myers implementation, not a crate — the
//! recovered edit script's op *order under ties* is part of the contract (the
//! forward and backtrack passes both use the exact tie-break
//! `k === -d || (k !== d && v[k-1] < v[k+1])`), so a library diff would be
//! plausible-but-unprovable.

use std::collections::BTreeSet;

/// A single edit operation over a line. `line` borrows from the input arrays
/// (`del`/`equal` from the old side, `ins` from the new side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineOp<'a> {
    pub op_type: LineOpType,
    pub line: &'a str,
}

/// The kind of an edit operation (v4's `'equal' | 'del' | 'ins'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineOpType {
    Equal,
    Del,
    Ins,
}

/// Myers O(ND) shortest-edit-script diff over two arrays of lines. Returns the
/// edit operations in forward order (v4 `diffLines`).
pub fn diff_lines<'a>(old_lines: &[&'a str], new_lines: &[&'a str]) -> Vec<LineOp<'a>> {
    let n = old_lines.len();
    let m = new_lines.len();

    if n == 0 && m == 0 {
        return Vec::new();
    }
    if n == 0 {
        return new_lines
            .iter()
            .map(|&line| LineOp {
                op_type: LineOpType::Ins,
                line,
            })
            .collect();
    }
    if m == 0 {
        return old_lines
            .iter()
            .map(|&line| LineOp {
                op_type: LineOpType::Del,
                line,
            })
            .collect();
    }

    let n_i = n as i64;
    let m_i = m as i64;
    let max = n_i + m_i;
    // v[k + offset] = furthest x reached along diagonal k for the current d.
    let offset = max;
    let mut v = vec![0i64; (2 * max + 1) as usize];
    let mut trace: Vec<Vec<i64>> = Vec::new();

    let idx = |k: i64| (k + offset) as usize;

    let mut found = false;
    let mut d = 0i64;
    while d <= max {
        // Snapshot v as it stood before this d's expansion, for backtracking.
        trace.push(v.clone());
        let mut k = -d;
        while k <= d {
            // Choose whether we arrived here by an insertion (down) or a deletion (right).
            let mut x: i64;
            if k == -d || (k != d && v[idx(k - 1)] < v[idx(k + 1)]) {
                x = v[idx(k + 1)];
            } else {
                x = v[idx(k - 1)] + 1;
            }
            let mut y = x - k;
            // Follow the diagonal of matching lines (a "snake").
            while x < n_i && y < m_i && old_lines[x as usize] == new_lines[y as usize] {
                x += 1;
                y += 1;
            }
            v[idx(k)] = x;
            if x >= n_i && y >= m_i {
                found = true;
                break;
            }
            k += 2;
        }
        if found {
            break;
        }
        d += 1;
    }

    // Backtrack from (n, m) to (0, 0), recovering the edit script in reverse.
    let mut ops: Vec<LineOp<'a>> = Vec::new();
    let mut x = n_i;
    let mut y = m_i;
    for d in (0..trace.len() as i64).rev() {
        let v_prev = &trace[d as usize];
        let k = x - y;
        let prev_k = if k == -d || (k != d && v_prev[idx(k - 1)] < v_prev[idx(k + 1)]) {
            k + 1
        } else {
            k - 1
        };
        let prev_x = v_prev[idx(prev_k)];
        let prev_y = prev_x - prev_k;

        // Diagonal moves are unchanged lines.
        while x > prev_x && y > prev_y {
            ops.push(LineOp {
                op_type: LineOpType::Equal,
                line: old_lines[(x - 1) as usize],
            });
            x -= 1;
            y -= 1;
        }
        // The single non-diagonal move into this d (skip at d === 0, the origin).
        if d > 0 {
            if x == prev_x {
                ops.push(LineOp {
                    op_type: LineOpType::Ins,
                    line: new_lines[(y - 1) as usize],
                });
            } else {
                ops.push(LineOp {
                    op_type: LineOpType::Del,
                    line: old_lines[(x - 1) as usize],
                });
            }
        }
        x = prev_x;
        y = prev_y;
    }

    ops.reverse();
    ops
}

/// Given a baseline and a current array of block texts, return the set of
/// *current* block indices that are genuinely new or modified — i.e. the blocks
/// a change gutter should mark (v4 `changedBlockIndices`).
///
/// A modification manifests as a deletion of the old text paired with an
/// insertion of the new text, so the inserted (current) block is the one marked.
/// Pure deletions have no counterpart in the current document and therefore mark
/// nothing — matching how a unified diff shows a removed line as a `-` with no
/// `+`. Content that merely shifted position stays `equal` and is not marked.
///
/// Returns a [`BTreeSet`] so the marked indices come out in ascending order
/// (v4's `Set` iteration order is insertion order, but every consumer treats it
/// as an unordered membership set).
pub fn changed_block_indices(baseline: &[&str], current: &[&str]) -> BTreeSet<usize> {
    let mut changed = BTreeSet::new();
    let mut current_index = 0usize;
    for op in diff_lines(baseline, current) {
        match op.op_type {
            LineOpType::Equal => current_index += 1,
            LineOpType::Ins => {
                changed.insert(current_index);
                current_index += 1;
            }
            // 'del' consumes a baseline-only block; no current index to advance or mark.
            LineOpType::Del => {}
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops<'a>(v: &[LineOp<'a>]) -> Vec<(LineOpType, &'a str)> {
        v.iter().map(|o| (o.op_type, o.line)).collect()
    }

    #[test]
    fn identical_input_all_equal() {
        assert_eq!(
            ops(&diff_lines(&["a", "b"], &["a", "b"])),
            vec![(LineOpType::Equal, "a"), (LineOpType::Equal, "b")]
        );
    }

    #[test]
    fn single_insertion_leaves_shifted_equal() {
        assert_eq!(
            ops(&diff_lines(&["a", "b", "c"], &["a", "NEW", "b", "c"])),
            vec![
                (LineOpType::Equal, "a"),
                (LineOpType::Ins, "NEW"),
                (LineOpType::Equal, "b"),
                (LineOpType::Equal, "c"),
            ]
        );
    }

    #[test]
    fn modification_is_del_paired_with_ins() {
        let o = diff_lines(&["a", "b", "c"], &["a", "B", "c"]);
        let dels: Vec<_> = o
            .iter()
            .filter(|x| x.op_type == LineOpType::Del)
            .map(|x| x.line)
            .collect();
        let inss: Vec<_> = o
            .iter()
            .filter(|x| x.op_type == LineOpType::Ins)
            .map(|x| x.line)
            .collect();
        assert_eq!(dels, vec!["b"]);
        assert_eq!(inss, vec!["B"]);
    }

    #[test]
    fn changed_blocks_marks_only_inserted() {
        let baseline = ["intro", "section one", "body one"];
        let current = ["intro", "NEW SECTION", "section one", "body one"];
        let got: Vec<usize> = changed_block_indices(&baseline, &current)
            .into_iter()
            .collect();
        assert_eq!(got, vec![1]);
    }

    #[test]
    fn changed_blocks_pure_delete_marks_nothing() {
        let got = changed_block_indices(&["a", "b", "c"], &["a", "c"]);
        assert!(got.is_empty());
    }
}
