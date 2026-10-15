//! P4.D292 (Tier 2 #19) — the bug-243 probe, through the REAL `Host::start`:
//! v4's bug 243 (`4ae0c4ca6`) was its FORKED job child running jobs before its
//! plugin registry had loaded, so `generateBuiltinEmbedding →
//! providerRegistry.createEmbeddingProvider('BUILTIN')` threw `Provider
//! 'BUILTIN' not found in registry` and a BUILTIN instance's boot help-doc
//! sections were skipped. v5 cannot have the race (handlers run in-process;
//! BUILTIN dispatches to `generate_builtin_embedding` before any registry
//! lookup; the TF-IDF code is linked, never loaded) — the NO-PORT P4.D303
//! ratifies on this lane's evidence. This test is the behavioural half of that
//! evidence.
//!
//! A fresh `setup` instance (its default embedding profile IS BUILTIN) with
//! memories planted boots: PHASE 3.8 finds the vocabulary unfitted and
//! enqueues the refit, the refit fits (help docs included in the corpus) and
//! enqueues the full reindex, and the reindex's EMBEDDING_GENERATE jobs embed
//! every help doc through BUILTIN. Asserted: every `help_docs` row ends with a
//! vector, and no `not found in registry` line appears anywhere in the boot.
//!
//! Run:
//!   cargo test -p quilltap-host --test host_boot_p4d292_builtin_help_embed

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use quilltap_core::db::Writer;
use quilltap_core::services::provisioning::provision_fresh_instance;
use quilltap_core::test_support::CaptureLayer;
use quilltap_host::spine::ProductionSpineFactory;
use quilltap_host::{Host, HostConfig};
use tracing_subscriber::layer::SubscriberExt;

const PEPPER: &str = "3q2+796tvu/erb7v3q2+796tvu/erb7v3q2+796tvu8=";

fn hermetic_config(base: &Path) -> HostConfig {
    let mut config = HostConfig::new(base);
    config.instances_path = Some(base.join("instances.json"));
    config.env_pepper = Some(PEPPER.to_string());
    config.autonomous_tick_ms = 3_600_000;
    config.stuck_check_ms = 3_600_000;
    config.terminal = false;
    config.seed_sample_content = false;
    // The production spine registers the EMBEDDING_GENERATE / EMBEDDING_REFIT /
    // EMBEDDING_REINDEX_ALL handlers the chain runs through.
    config.spine = Some(Arc::new(ProductionSpineFactory::new(
        base.to_path_buf(),
        "0.0.0-test".to_string(),
        "UTC".to_string(),
        config.display_zone.clone(),
    )));
    config
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_fresh_builtin_instance_embeds_every_help_doc_with_no_registry_miss() {
    let logs = Arc::new(Mutex::new(Vec::<String>::new()));
    tracing::subscriber::set_global_default(
        tracing_subscriber::registry().with(CaptureLayer(logs.clone())),
    )
    .expect("this binary owns the global subscriber");

    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    std::fs::create_dir_all(&data).unwrap();
    provision_fresh_instance(&data, PEPPER).expect("provision");
    {
        let w = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
        w.connection()
            .execute_batch(
                r#"INSERT INTO "memories" ("id","characterId","content","summary","createdAt","updatedAt") VALUES
                 ('e2920000-0000-4000-8000-000000000a01','e2920000-0000-4000-8000-0000000000c1','The lantern burns in the Salon window.','A lantern','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z'),
                 ('e2920000-0000-4000-8000-000000000a02','e2920000-0000-4000-8000-0000000000c1','The Commonplace Book keeps every memory.','A book','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z')"#,
            )
            .unwrap();
    }

    let host = Host::start(hermetic_config(dir.path())).expect("the boot");
    let db = host.core().db().expect("a ready engine");
    let counts = || -> (i64, i64) {
        db.read_main(|c| {
            Ok(c.query_row(
                "SELECT COUNT(*), SUM(CASE WHEN embedding IS NOT NULL AND length(embedding) > 0 THEN 1 ELSE 0 END) FROM help_docs",
                [],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?.unwrap_or(0))),
            )?)
        })
        .unwrap()
    };
    let start = Instant::now();
    loop {
        let (total, embedded) = counts();
        if total > 0 && embedded == total {
            break;
        }
        assert!(
            start.elapsed() < Duration::from_secs(120),
            "help docs never all embedded: {embedded} of {total}"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let lines = logs.lock().unwrap().clone();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("TF-IDF vocabulary refit enqueued")),
        "the PHASE 3.8 refit started the chain"
    );
    let misses: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("not found in registry"))
        .collect();
    assert!(misses.is_empty(), "a registry miss appeared: {misses:#?}");
    drop(host);
}
