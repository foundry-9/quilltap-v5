//! Consolidation clustering — pure, I/O-free. v4
//! `lib/memory/consolidation-clustering.ts` (`d58548051`, byte-identical at the
//! `01a83539d` pin).
//!
//! The consolidation job ([`crate::services::consolidation`]) folds clusters of
//! a character's hot memories into digest rows. This module decides *which*
//! rows belong together; it never reads the database, calls a model, or logs,
//! so it is trivially unit-testable.
//!
//! Within one subject bucket (memory-consolidation-and-tiers.md §C3):
//!
//!   1. Semantic and episodic rows never share a cluster — episodes consolidate
//!      into *episode* digests, never into standing-fact digests.
//!   2. **Digest attraction.** A row whose best neighbour (cosine ≥ threshold,
//!      across every item of its kind, digests included) is an existing hot
//!      digest joins that digest's cluster, closest first, until the cluster
//!      reaches `maxClusterSize` (the digest counts as one). Digests are seeds:
//!      two digests are never merged with each other.
//!   3. **Greedy agglomerative clustering** over the remaining rows with
//!      *average linkage*: repeatedly merge the most similar pair of clusters
//!      whose average pairwise cosine is ≥ threshold and whose merged size is
//!      ≤ `maxClusterSize`. Similarities are updated with the Lance–Williams
//!      formula, and each cluster caches its best eligible partner, so a bucket
//!      of `n` rows costs O(n²) rather than O(n³).
//!   4. **Episodic window.** An episodic cluster may only span
//!      [`EPISODIC_WINDOW_MS`] (one day) between its earliest and latest event
//!      time — i.e. every pair of members sits within a day of each other.
//!      Event time is `occurredAt`, falling back to `createdAt` (the caller
//!      supplies it). This mirrors the gate's `DATE_GUARD_DAYS` rule — distinct
//!      occasions stay distinct — at a one-day grain, and like the date guard
//!      it never blocks on a missing timestamp.
//!
//! [`select_clusters`] then applies the size floor and the run budget.
//!
//! **f32 fidelity (P4.D271 R-H).** v4 keeps the pairwise cosines in a
//! `Float32Array`: the dot is summed in f64 over the f32 elements (JS promotes),
//! STORED rounded to f32, and every comparison reads the stored f32 promoted
//! back to f64. The Lance–Williams average is computed in f64 from the stored
//! f32s and stored back as f32. Every strict `>` keeps the first index on a tie,
//! and both sorts are stable. Tier-1 exact against v4's REAL exports
//! (`consolidation_clustering_equivalence`).

use std::collections::HashMap;

/// Milliseconds per day.
const DAY_MS: f64 = 86_400_000.0;

/// Episodic rows cluster only with episodic rows whose event times sit within
/// this window.
pub const EPISODIC_WINDOW_MS: f64 = DAY_MS;

/// The two memory kinds the clusterer separates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClusterKind {
    Semantic,
    Episodic,
}

impl ClusterKind {
    /// v4's wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            ClusterKind::Semantic => "semantic",
            ClusterKind::Episodic => "episodic",
        }
    }
}

/// One row (or existing digest) offered to the clusterer.
#[derive(Debug, Clone)]
pub struct ClusterItem {
    pub id: String,
    /// Unit-length embedding (cosine == dot product) — the stored BLOB's f32s.
    pub embedding: Vec<f32>,
    pub kind: ClusterKind,
    /// True for an existing hot digest (`source: 'CONSOLIDATED'`).
    pub is_digest: bool,
    /// Event time in ms (`occurredAt`, else `createdAt`); `None` when unknown.
    pub event_time_ms: Option<f64>,
    /// Write time in ms — drives the age-based size floor.
    pub created_at_ms: f64,
    /// `consolidatedAt` in ms, or `None` when the consolidator has never
    /// considered the row.
    pub considered_at_ms: Option<f64>,
    /// Ranking weight — the row's `reinforcedImportance`.
    pub weight: f64,
}

/// v4 `ClusterParams`.
#[derive(Debug, Clone, Copy)]
pub struct ClusterParams {
    /// Cosine similarity a pair (or cluster average) must reach to merge.
    pub threshold: f64,
    /// Largest cluster, counting an existing digest as one member.
    pub max_cluster_size: f64,
    /// Override for tests; defaults to [`EPISODIC_WINDOW_MS`].
    pub episodic_window_ms: Option<f64>,
}

/// A cluster: an optional existing digest plus the rows that join it.
#[derive(Debug, Clone, PartialEq)]
pub struct Cluster {
    pub kind: ClusterKind,
    /// The existing digest this cluster formed around, or `None`.
    pub digest_id: Option<String>,
    /// Non-digest member ids.
    pub member_ids: Vec<String>,
    /// Σ weight of every member, digest included.
    pub score: f64,
}

/// Anything [`select_clusters`] can rank — v4's `C extends Cluster` (the
/// service's clusters carry their bucket key alongside).
pub trait AsCluster {
    fn as_cluster(&self) -> &Cluster;
}

impl AsCluster for Cluster {
    fn as_cluster(&self) -> &Cluster {
        self
    }
}

/// Cosine of two unit vectors; -1 when their dimensions disagree (never
/// clusters). The dot is summed in f64 over the promoted f32 elements, as JS
/// does.
pub fn cosine(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return -1.0;
    }
    let mut dot = 0.0f64;
    for i in 0..a.len() {
        dot += a[i] as f64 * b[i] as f64;
    }
    dot
}

#[derive(Debug, Clone, Copy)]
struct Span {
    min: Option<f64>,
    max: Option<f64>,
}

/// JS `Math.min(...xs)` / `Math.max(...xs)` over the non-null ends (a NaN
/// propagates, as in JS).
fn js_min2(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.min(b)
    }
}

fn js_max2(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else {
        a.max(b)
    }
}

fn merge_span(a: Span, b: Span) -> Span {
    let min = match (a.min, b.min) {
        (Some(x), Some(y)) => Some(js_min2(x, y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };
    let max = match (a.max, b.max) {
        (Some(x), Some(y)) => Some(js_max2(x, y)),
        (Some(x), None) | (None, Some(x)) => Some(x),
        (None, None) => None,
    };
    Span { min, max }
}

fn span_fits(span: Span, window_ms: f64) -> bool {
    match (span.min, span.max) {
        (Some(min), Some(max)) => max - min <= window_ms,
        _ => true,
    }
}

/// Cluster one subject bucket. Returns every cluster, singletons included (the
/// caller needs them to mark lone rows considered). Digest seeds that attracted
/// no rows are omitted.
pub fn cluster_bucket(items: &[ClusterItem], params: &ClusterParams) -> Vec<Cluster> {
    let mut out = Vec::new();
    for kind in [ClusterKind::Semantic, ClusterKind::Episodic] {
        let group: Vec<&ClusterItem> = items.iter().filter(|item| item.kind == kind).collect();
        if group.is_empty() {
            continue;
        }
        out.extend(cluster_group(&group, kind, params));
    }
    out
}

fn cluster_group(
    group: &[&ClusterItem],
    kind: ClusterKind,
    params: &ClusterParams,
) -> Vec<Cluster> {
    let n = group.len();
    let threshold = params.threshold;
    let max_size = js_max2(1.0, params.max_cluster_size);
    let window_ms = params.episodic_window_ms.unwrap_or(EPISODIC_WINDOW_MS);
    let timed = kind == ClusterKind::Episodic;

    // Full pairwise similarity. Rows of a bucket are capped by the caller, so
    // n² floats stay small.
    let mut sim = vec![0f32; n * n];
    for i in 0..n {
        sim[i * n + i] = 1.0;
        for j in (i + 1)..n {
            let s = cosine(&group[i].embedding, &group[j].embedding) as f32;
            sim[i * n + j] = s;
            sim[j * n + i] = s;
        }
    }

    let pair_time_ok = |i: usize, j: usize| -> bool {
        if !timed {
            return true;
        }
        match (group[i].event_time_ms, group[j].event_time_ms) {
            (Some(a), Some(b)) => (a - b).abs() <= window_ms,
            _ => true,
        }
    };

    // ── Step 1: digest attraction ────────────────────────────────────────────
    let mut digest_idx: Vec<usize> = Vec::new();
    let mut row_idx: Vec<usize> = Vec::new();
    for (i, item) in group.iter().enumerate() {
        if item.is_digest {
            digest_idx.push(i);
        } else {
            row_idx.push(i);
        }
    }

    struct Attraction {
        row: usize,
        digest: usize,
        s: f64,
    }
    let mut attractions: Vec<Attraction> = Vec::new();
    for &i in &row_idx {
        let mut best: Option<usize> = None;
        let mut best_s = f64::NEG_INFINITY;
        for j in 0..n {
            if j == i {
                continue;
            }
            let s = sim[i * n + j] as f64;
            if s < threshold || !pair_time_ok(i, j) {
                continue;
            }
            if s > best_s {
                best_s = s;
                best = Some(j);
            }
        }
        if let Some(b) = best {
            if group[b].is_digest {
                attractions.push(Attraction {
                    row: i,
                    digest: b,
                    s: best_s,
                });
            }
        }
    }
    // JS `sort((a, b) => b.s - a.s)` — stable, descending.
    attractions.sort_by(|a, b| js_compare_desc(a.s, b.s));

    let mut digest_members: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut digest_span: HashMap<usize, Span> = HashMap::new();
    for &d in &digest_idx {
        digest_members.insert(d, Vec::new());
        let t = group[d].event_time_ms;
        digest_span.insert(d, Span { min: t, max: t });
    }
    let mut attracted = vec![false; n];
    for a in &attractions {
        let members_len = digest_members[&a.digest].len();
        if (members_len as f64) + 1.0 >= max_size {
            continue; // digest + members must stay ≤ maxSize
        }
        let t = group[a.row].event_time_ms;
        let merged = merge_span(digest_span[&a.digest], Span { min: t, max: t });
        if timed && !span_fits(merged, window_ms) {
            continue;
        }
        digest_members
            .get_mut(&a.digest)
            .expect("seeded")
            .push(a.row);
        digest_span.insert(a.digest, merged);
        attracted[a.row] = true;
    }

    let mut result: Vec<Cluster> = Vec::new();
    for &d in &digest_idx {
        let rows = &digest_members[&d];
        if rows.is_empty() {
            continue;
        }
        let mut sum = 0.0f64;
        for &i in rows {
            sum += group[i].weight;
        }
        result.push(Cluster {
            kind,
            digest_id: Some(group[d].id.clone()),
            member_ids: rows.iter().map(|&i| group[i].id.clone()).collect(),
            score: group[d].weight + sum,
        });
    }

    // ── Step 2: average-linkage agglomeration over the rest ──────────────────
    let rest: Vec<usize> = row_idx.iter().copied().filter(|&i| !attracted[i]).collect();
    let m = rest.len();
    if m == 0 {
        return result;
    }

    let mut mm = vec![0f32; m * m];
    for a in 0..m {
        for b in 0..m {
            mm[a * m + b] = sim[rest[a] * n + rest[b]];
        }
    }
    let mut active = vec![true; m];
    let mut size = vec![1.0f64; m];
    let mut members: Vec<Vec<usize>> = rest.iter().map(|&i| vec![i]).collect();
    let mut spans: Vec<Span> = rest
        .iter()
        .map(|&i| Span {
            min: group[i].event_time_ms,
            max: group[i].event_time_ms,
        })
        .collect();
    let mut best: Vec<Option<usize>> = vec![None; m];
    let mut best_val: Vec<f64> = vec![f64::NEG_INFINITY; m];

    let eligible =
        |a: usize, b: usize, active: &[bool], size: &[f64], mm: &[f32], spans: &[Span]| -> bool {
            if a == b || !active[a] || !active[b] {
                return false;
            }
            if size[a] + size[b] > max_size {
                return false;
            }
            if (mm[a * m + b] as f64) < threshold {
                return false;
            }
            if timed && !span_fits(merge_span(spans[a], spans[b]), window_ms) {
                return false;
            }
            true
        };

    let recompute = |a: usize,
                     best: &mut [Option<usize>],
                     best_val: &mut [f64],
                     active: &[bool],
                     size: &[f64],
                     mm: &[f32],
                     spans: &[Span]| {
        best[a] = None;
        best_val[a] = f64::NEG_INFINITY;
        for b in 0..m {
            if !eligible(a, b, active, size, mm, spans) {
                continue;
            }
            let v = mm[a * m + b] as f64;
            if v > best_val[a] {
                best_val[a] = v;
                best[a] = Some(b);
            }
        }
    };

    for a in 0..m {
        recompute(a, &mut best, &mut best_val, &active, &size, &mm, &spans);
    }

    loop {
        let mut pick: Option<usize> = None;
        let mut top = f64::NEG_INFINITY;
        for i in 0..m {
            if active[i] && best[i].is_some() && best_val[i] > top {
                top = best_val[i];
                pick = Some(i);
            }
        }
        let Some(a) = pick else { break };
        let b = best[a].expect("picked with a best");
        if !eligible(a, b, &active, &size, &mm, &spans) {
            // Stale cache: a size or span constraint closed this pair since it was
            // cached. Constraints only tighten, so a recompute is always progress.
            recompute(a, &mut best, &mut best_val, &active, &size, &mm, &spans);
            continue;
        }

        // Merge b into a — Lance–Williams update for average linkage.
        let sa = size[a];
        let sb = size[b];
        for c in 0..m {
            if !active[c] || c == a || c == b {
                continue;
            }
            let v = ((sa * mm[a * m + c] as f64 + sb * mm[b * m + c] as f64) / (sa + sb)) as f32;
            mm[a * m + c] = v;
            mm[c * m + a] = v;
        }
        active[b] = false;
        size[a] = sa + sb;
        let moved = std::mem::take(&mut members[b]);
        members[a].extend(moved);
        spans[a] = merge_span(spans[a], spans[b]);

        // Average linkage is reducible: a merged cluster is never closer to a third
        // cluster than the nearer of its parts was, so only rows whose cached best
        // was a or b can have changed.
        recompute(a, &mut best, &mut best_val, &active, &size, &mm, &spans);
        for c in 0..m {
            if !active[c] || c == a {
                continue;
            }
            if best[c] == Some(a) || best[c] == Some(b) {
                recompute(c, &mut best, &mut best_val, &active, &size, &mm, &spans);
            }
        }
    }

    for a in 0..m {
        if !active[a] {
            continue;
        }
        let ids = members[a].iter().map(|&i| group[i].id.clone()).collect();
        let mut score = 0.0f64;
        for &i in &members[a] {
            score += group[i].weight;
        }
        result.push(Cluster {
            kind,
            digest_id: None,
            member_ids: ids,
            score,
        });
    }
    result
}

/// A JS `sort((a, b) => b - a)` comparator: a NaN difference reads as "equal",
/// which a stable sort leaves in place.
fn js_compare_desc(a: f64, b: f64) -> std::cmp::Ordering {
    let d = b - a;
    if d > 0.0 {
        std::cmp::Ordering::Greater
    } else if d < 0.0 {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    }
}

/// What [`select_clusters`] decided for each cluster.
#[derive(Debug, Clone)]
pub struct ClusterSelection<C> {
    /// Qualified clusters within the run budget, best score first.
    pub selected: Vec<C>,
    /// Qualified clusters past the budget — left for the next run, untouched.
    pub deferred: Vec<C>,
    /// Clusters below the size floor; their never-considered rows get
    /// `consolidatedAt`.
    pub below_min: Vec<C>,
    /// Clusters with nothing new since every member was last considered —
    /// skipped silently.
    pub stale: Vec<C>,
}

/// v4 `SelectClustersOptions`.
#[derive(Debug, Clone, Copy)]
pub struct SelectClustersOptions {
    pub min_cluster_size: f64,
    pub mature_after_days: f64,
    pub max_clusters: f64,
    pub now_ms: f64,
}

/// The size floor for one cluster.
///
/// The spec says `minClusterSize` drops to 2 "when any member is >
/// matureAfterDays old" — but only rows older than `matureAfterDays` are ever
/// candidates, so read literally the drop would always apply. We read it as
/// intended: a row that has sat a *further* full maturity window past
/// eligibility (older than 2 × `matureAfterDays`) without gathering a third
/// neighbour has had its chance, so a pair is enough for it.
pub fn effective_min_cluster_size(
    member_created_at_ms: &[f64],
    min_cluster_size: f64,
    mature_after_days: f64,
    now_ms: f64,
) -> f64 {
    let floor = js_max2(2.0, min_cluster_size.floor());
    let long_ago_ms = now_ms - 2.0 * mature_after_days * DAY_MS;
    if member_created_at_ms.iter().any(|&t| t < long_ago_ms) {
        2.0
    } else {
        floor
    }
}

/// Whether a cluster carries anything new since its members were last
/// considered: a never-considered row, or a row written after another member
/// was last considered (a newer neighbour arrived — `consolidatedAt` compare).
pub fn cluster_has_new_material(members: &[&ClusterItem]) -> bool {
    if members.iter().any(|m| m.considered_at_ms.is_none()) {
        return true;
    }
    // `Math.min(...considered)` — Infinity over an empty list, NaN-propagating.
    let mut oldest_considered = f64::INFINITY;
    for m in members {
        oldest_considered = js_min2(
            oldest_considered,
            m.considered_at_ms.expect("all considered"),
        );
    }
    members.iter().any(|m| m.created_at_ms > oldest_considered)
}

/// Apply the size floor, the "anything new?" check, and the per-run budget:
/// qualified clusters are ordered by total member `reinforcedImportance` and the
/// top `maxClusters` are selected.
pub fn select_clusters<C: AsCluster + Clone>(
    clusters: &[C],
    items_by_id: &HashMap<String, ClusterItem>,
    opts: &SelectClustersOptions,
) -> ClusterSelection<C> {
    let mut qualified: Vec<C> = Vec::new();
    let mut below_min: Vec<C> = Vec::new();
    let mut stale: Vec<C> = Vec::new();

    for cluster in clusters {
        let c = cluster.as_cluster();
        let rows: Vec<&ClusterItem> = c
            .member_ids
            .iter()
            .filter_map(|id| items_by_id.get(id))
            .collect();
        if !cluster_has_new_material(&rows) {
            stale.push(cluster.clone());
            continue;
        }
        let size = rows.len() as f64 + if c.digest_id.is_some() { 1.0 } else { 0.0 };
        let created: Vec<f64> = rows.iter().map(|r| r.created_at_ms).collect();
        let floor = effective_min_cluster_size(
            &created,
            opts.min_cluster_size,
            opts.mature_after_days,
            opts.now_ms,
        );
        if size < floor {
            below_min.push(cluster.clone());
            continue;
        }
        qualified.push(cluster.clone());
    }

    qualified.sort_by(|a, b| js_compare_desc(a.as_cluster().score, b.as_cluster().score));
    // `Math.max(0, Math.floor(maxClusters))` then `slice(0, budget)`: a NaN
    // budget slices nothing; one past the length slices everything.
    let budget = js_max2(0.0, opts.max_clusters.floor());
    let cut = if budget.is_nan() {
        0
    } else if budget >= qualified.len() as f64 {
        qualified.len()
    } else {
        budget as usize
    };
    let deferred = qualified.split_off(cut);
    ClusterSelection {
        selected: qualified,
        deferred,
        below_min,
        stale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, e: &[f32], digest: bool) -> ClusterItem {
        ClusterItem {
            id: id.into(),
            embedding: e.to_vec(),
            kind: ClusterKind::Semantic,
            is_digest: digest,
            event_time_ms: None,
            created_at_ms: 0.0,
            considered_at_ms: None,
            weight: 0.5,
        }
    }

    #[test]
    fn cosine_mismatch_is_minus_one() {
        assert_eq!(cosine(&[1.0], &[1.0, 0.0]), -1.0);
        assert_eq!(cosine(&[], &[]), -1.0);
    }

    #[test]
    fn digest_attracts_and_rows_merge() {
        let items = vec![
            item("d", &[1.0, 0.0], true),
            item("a", &[1.0, 0.0], false),
            item("b", &[0.0, 1.0], false),
            item("c", &[0.0, 1.0], false),
        ];
        let p = ClusterParams {
            threshold: 0.72,
            max_cluster_size: 30.0,
            episodic_window_ms: None,
        };
        let out = cluster_bucket(&items, &p);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].digest_id.as_deref(), Some("d"));
        assert_eq!(out[0].member_ids, vec!["a"]);
        assert_eq!(out[1].member_ids, vec!["b", "c"]);
    }

    #[test]
    fn empty_members_are_stale() {
        assert!(!cluster_has_new_material(&[]));
    }
}
