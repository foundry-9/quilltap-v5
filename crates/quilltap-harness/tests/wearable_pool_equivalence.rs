//! Tier-1 differential (P4.D279, keystone C1 §1): the Wearable Pool's PURE
//! surface — `quilltap_core::wearable_pool::{build_wearable_pool,
//! component_graph, merge_wearable_tiers, merge_wearable_pool}` vs v4's REAL
//! `lib/wardrobe/pool.ts` (`buildWearablePool`, `componentGraph`) and
//! `lib/wardrobe/wearable-pool.ts` (`mergeWearableTiers`, `mergeWearablePool`)
//! at `4a38222e6`.
//!
//! The corpus is `__tests__/unit/lib/wardrobe/pool.test.ts`'s cases made pure
//! (the loader's tiers handed straight to the builder) plus the three
//! `buildWearablePool` cases of `archived-never-auditions.test.ts`. Every row
//! carries its four layers (items already origin-tagged, as the loader tags
//! them) and the WHOLE battery v4 answered over the pool built from them; this
//! test rebuilds the identical pool and compares each surface EXACTLY:
//!
//!   - `byId` as the ORDERED array (`by_id_ordered()` vs `Array.from(pool.byId
//!     .values())` — a shadowing item keeps the weaker tier's position);
//!   - `wearable()` + a second call (memoised: same list);
//!   - `get(id)` per probe, `getMany(ids)` with unknowns and duplicates;
//!   - `findByTitle(q)` (own-first archived-included, JS-trimmed, case-folded,
//!     an archived shared never found, blank → none);
//!   - `owns(item)` for every pooled item + the foreign probes;
//!   - `componentGraph(pool, roots)` as the ORDERED id list + items;
//!   - `mergeWearableTiers` / `mergeWearablePool` over the same layers.
//!
//! RED-FIRST: this family cannot be red on pre-keystone main — `build_wearable_
//! pool` did not exist there (the module is new in P4.D279); recorded, not
//! measured.
//!
//! Generate the oracle output (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5W=${V5W:-$HOME/source/quilltap-v5}
//!   cd ~/source/quilltap-server
//!   $N/node --import tsx $V5W/harness/oracle/cases/wearable-pool.ts \
//!     > /tmp/oracle-wearable-pool.ndjson
//! Run:
//!   QT_ORACLE_WEARABLE_POOL=/tmp/oracle-wearable-pool.ndjson \
//!     cargo test -p quilltap-harness --test wearable_pool_equivalence -- --nocapture

use quilltap_core::wardrobe_tiers::SharedWardrobeTiers;
use quilltap_core::wearable_pool::{
    build_wearable_pool, component_graph, merge_wearable_pool, merge_wearable_tiers,
};
use serde::Deserialize;
use serde_json::{json, Value};

const BASELINE: &str = "p4.d279-wearable-pool";

#[derive(Deserialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Row {
    Meta {
        baseline: String,
    },
    Case {
        name: String,
        #[serde(rename = "characterId")]
        character_id: String,
        tiers: Value,
        layers: Layers,
        #[serde(rename = "poolTiers")]
        pool_tiers: Value,
        #[serde(rename = "byId")]
        by_id: Vec<Value>,
        #[serde(rename = "byIdSize")]
        by_id_size: usize,
        wearable: Vec<Value>,
        #[serde(rename = "wearableMemoised")]
        wearable_memoised: bool,
        #[serde(rename = "wearableAgain")]
        wearable_again: Vec<Value>,
        get: Vec<GetProbe>,
        #[serde(rename = "getManyIds")]
        get_many_ids: Vec<String>,
        #[serde(rename = "getMany")]
        get_many: Vec<Value>,
        #[serde(rename = "findByTitle")]
        find_by_title: Vec<TitleProbe>,
        owns: Vec<OwnsProbe>,
        #[serde(rename = "componentGraph")]
        component_graph: Vec<GraphProbe>,
        #[serde(rename = "mergeWearableTiers")]
        merge_tiers: Vec<Value>,
        #[serde(rename = "mergeWearablePool")]
        merge_pool: Vec<Value>,
    },
}

#[derive(Deserialize)]
struct Layers {
    own: Vec<Value>,
    group: Vec<Value>,
    project: Vec<Value>,
    general: Vec<Value>,
}

#[derive(Deserialize)]
struct GetProbe {
    id: String,
    item: Value,
}

#[derive(Deserialize)]
struct TitleProbe {
    query: String,
    item: Value,
}

#[derive(Deserialize)]
struct OwnsProbe {
    probe: Value,
    owns: bool,
}

#[derive(Deserialize)]
struct GraphProbe {
    roots: Vec<String>,
    ids: Vec<String>,
    items: Vec<Value>,
}

fn tiers_of(v: &Value) -> SharedWardrobeTiers {
    let list = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    SharedWardrobeTiers {
        group_mount_point_ids: list("groupMountPointIds"),
        project_mount_point_ids: list("projectMountPointIds"),
    }
}

fn tiers_json(t: &SharedWardrobeTiers) -> Value {
    json!({
        "groupMountPointIds": t.group_mount_point_ids,
        "projectMountPointIds": t.project_mount_point_ids,
    })
}

fn id_of(item: &Value) -> &str {
    item.get("id").and_then(Value::as_str).unwrap_or_default()
}

#[test]
fn wearable_pool_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_WEARABLE_POOL") else {
        eprintln!("SKIP: set QT_ORACLE_WEARABLE_POOL to the oracle NDJSON (see test header).");
        return;
    };
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {path}: {e}"));

    let mut saw_meta = false;
    let mut cases = 0;
    // Shape witnesses: the corpus must exercise every rule, not only agree.
    let (mut shadowed_rows, mut unmask_rows, mut archived_own_title_hits) = (0, 0, 0);
    let (mut title_misses, mut cycle_graphs, mut owns_true, mut owns_false_with_char_id) =
        (0, 0, 0, 0);

    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        match serde_json::from_str::<Row>(line).expect("oracle row") {
            Row::Meta { baseline } => {
                assert_eq!(
                    baseline, BASELINE,
                    "the oracle NDJSON predates this lane — regenerate it"
                );
                saw_meta = true;
            }
            Row::Case {
                name,
                character_id,
                tiers,
                layers,
                pool_tiers,
                by_id,
                by_id_size,
                wearable,
                wearable_memoised,
                wearable_again,
                get,
                get_many_ids,
                get_many,
                find_by_title,
                owns,
                component_graph: graphs,
                merge_tiers,
                merge_pool,
            } => {
                let pool = build_wearable_pool(
                    &character_id,
                    tiers_of(&tiers),
                    layers.own.clone(),
                    layers.group.clone(),
                    layers.project.clone(),
                    layers.general.clone(),
                );

                // tiers travel through untouched.
                assert_eq!(tiers_json(&pool.tiers), pool_tiers, "[{name}] tiers");
                assert_eq!(pool.character_id, character_id, "[{name}] characterId");

                // byId — ORDERED, archived included, shadow in place.
                let got_by_id: Vec<Value> = pool.by_id_ordered().cloned().collect();
                assert_eq!(got_by_id, by_id, "[{name}] byId (ordered)");
                assert_eq!(pool.reachable(), by_id_size, "[{name}] byId.size");
                let total: usize = layers.own.len()
                    + layers.group.len()
                    + layers.project.len()
                    + layers.general.len();
                if total > by_id_size {
                    shadowed_rows += 1;
                }

                // wearable() — memoised.
                assert!(
                    wearable_memoised,
                    "[{name}] v4's wearable() must be memoised"
                );
                assert_eq!(pool.wearable(), wearable.as_slice(), "[{name}] wearable()");
                assert_eq!(
                    pool.wearable(),
                    wearable_again.as_slice(),
                    "[{name}] wearable() second call"
                );
                // The unmask witness: an id whose byId value is archived but
                // whose wearable value is live (a weaker live copy resurfaced).
                for w in &wearable {
                    if let Some(b) = pool.get(id_of(w)) {
                        if quilltap_core::wearable_pool::is_archived_truthy(b)
                            && !quilltap_core::wearable_pool::is_archived_truthy(w)
                        {
                            unmask_rows += 1;
                        }
                    }
                }

                // get(id).
                for p in &get {
                    let got = pool.get(&p.id).cloned().unwrap_or(Value::Null);
                    assert_eq!(got, p.item, "[{name}] get({})", p.id);
                }

                // getMany(ids).
                let got_many: Vec<Value> =
                    pool.get_many(&get_many_ids).into_iter().cloned().collect();
                assert_eq!(got_many, get_many, "[{name}] getMany({get_many_ids:?})");

                // findByTitle(q).
                for p in &find_by_title {
                    let got = pool.find_by_title(&p.query).cloned().unwrap_or(Value::Null);
                    assert_eq!(got, p.item, "[{name}] findByTitle({:?})", p.query);
                    if p.item.is_null() {
                        title_misses += 1;
                    } else if quilltap_core::wearable_pool::is_archived_truthy(&p.item) {
                        archived_own_title_hits += 1;
                    }
                }

                // owns(item).
                for p in &owns {
                    let got = pool.owns(&p.probe);
                    assert_eq!(got, p.owns, "[{name}] owns({})", p.probe);
                    if got {
                        owns_true += 1;
                    } else if p.probe.get("characterId").and_then(Value::as_str)
                        == Some(character_id.as_str())
                    {
                        owns_false_with_char_id += 1;
                    }
                }

                // componentGraph(pool, roots) — discovery order.
                for g in &graphs {
                    let got_items = component_graph(&pool, &g.roots);
                    let got_ids: Vec<String> =
                        got_items.iter().map(|i| id_of(i).to_string()).collect();
                    assert_eq!(got_ids, g.ids, "[{name}] componentGraph({:?}) ids", g.roots);
                    assert_eq!(
                        got_items, g.items,
                        "[{name}] componentGraph({:?}) items",
                        g.roots
                    );
                    let components: usize = got_items
                        .iter()
                        .filter_map(|i| i.get("componentItemIds").and_then(Value::as_array))
                        .map(Vec::len)
                        .sum();
                    if components > got_items.len() {
                        cycle_graphs += 1;
                    }
                }

                // wearable-pool.ts — the client-safe merge over the same layers.
                let got_tiers = merge_wearable_tiers(&[
                    &layers.general,
                    &layers.project,
                    &layers.group,
                    &layers.own,
                ]);
                assert_eq!(got_tiers, merge_tiers, "[{name}] mergeWearableTiers");
                let shared: Vec<Value> = layers
                    .general
                    .iter()
                    .chain(layers.project.iter())
                    .chain(layers.group.iter())
                    .cloned()
                    .collect();
                let got_pool = merge_wearable_pool(&shared, &layers.own);
                assert_eq!(got_pool, merge_pool, "[{name}] mergeWearablePool");

                cases += 1;
            }
        }
    }

    assert!(saw_meta, "the oracle carries no meta row");
    assert!(cases >= 24, "expected the full corpus, got {cases} cases");
    assert!(
        shadowed_rows >= 4,
        "corpus must shadow across tiers ({shadowed_rows})"
    );
    assert!(
        unmask_rows >= 3,
        "corpus must unmask a live copy under an archived one ({unmask_rows})"
    );
    assert!(
        archived_own_title_hits >= 1,
        "corpus must find an ARCHIVED own item by title ({archived_own_title_hits})"
    );
    assert!(
        title_misses >= 4,
        "corpus must miss by title ({title_misses})"
    );
    assert!(
        cycle_graphs >= 1,
        "corpus must walk a component cycle ({cycle_graphs})"
    );
    assert!(
        owns_true >= 3 && owns_false_with_char_id >= 2,
        "owns both ways ({owns_true}/{owns_false_with_char_id})"
    );

    eprintln!("OK: wearable-pool matched the oracle on {cases} cases.");
}
