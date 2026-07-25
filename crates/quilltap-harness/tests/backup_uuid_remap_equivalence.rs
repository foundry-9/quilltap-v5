//! P4.9G6 UUID-remap differential (**tier 1, exact**): runs
//! `quilltap_core::services::backup::uuid_remap::remap_backup_data` over the
//! committed corpus and diffs it against v4's REAL `remapBackupData`
//! (`lib/backup/restore/uuid-remap.ts:70`, the `backup-uuid-remap` oracle).
//!
//! ## No normalization at all — by design
//!
//! Both sides run the SAME deterministic id source
//! (`00000000-0000-4000-8000-<12-digit counter, from 1>`; the oracle mocks
//! `crypto.randomUUID`), so every minted id is directly comparable. If this test
//! ever needs a normalizer, the id source has come unpinned — fix that, not the
//! diff.
//!
//! ## What is compared
//!
//! 1. **Every collection, byte-level.** `serde_json` is built with
//!    `preserve_order`, so object key ORDER is observable — and key order is
//!    exactly what v4's three order traps (spread-then-`userId`, `delete` as a
//!    shift-remove, the `remapFields`→`remapArrayFields` chain) are about. Each
//!    collection is re-serialized on both sides and the strings compared.
//! 2. **The key SET of the returned object.** v4 returns 38 collections plus
//!    `manifest: data.manifest`, which is `undefined` here (v5's `BackupData`
//!    carries no manifest — that pass-through is the caller's business) and so
//!    is dropped by `JSON.stringify`. 38 keys, no more.
//! 3. **`getMapping()` / `mapping_object()`, entry for entry and in order.**
//!    That is the direct proof that cross-references stayed consistent: if any
//!    pass minted a second id for an already-seen key, the map diverges here
//!    even when the collections happen to agree.
//! 4. **`getSize()`**, as the cheap scalar summary of the same claim.
//!
//! ## The corpus is pinned by hash
//!
//! The oracle writes `harness/oracle/fixtures/uuid-remap-corpus.json` (the
//! INPUTS) and the NDJSON (v4's OUTPUTS) in one invocation, and stamps every
//! NDJSON line with the sha256 of the corpus bytes it wrote. This test
//! recomputes that hash from the COMMITTED file and refuses to run on a
//! mismatch — a stale corpus can never green-light a pass. Regenerate both
//! together (recipe in `harness/oracle/cases/backup-uuid-remap.test.ts`), then:
//!   QT_ORACLE_UUID_REMAP=/tmp/oracle-backup-uuid-remap.ndjson \
//!     cargo test -p quilltap-harness --test backup_uuid_remap_equivalence -- --nocapture

use std::path::PathBuf;

use quilltap_core::services::backup::collect::BackupData;
use quilltap_core::services::backup::uuid_remap::remap_backup_data;
use quilltap_core::services::backup::uuid_remapper::UuidRemapper;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Every collection `remapBackupData` returns, in v4's return-literal order —
/// and the ONLY keys it returns (`manifest` is `undefined` here and dropped).
const COLLECTIONS: [&str; 38] = [
    "characters",
    "chats",
    "tags",
    "connectionProfiles",
    "imageProfiles",
    "embeddingProfiles",
    "memories",
    "files",
    "promptTemplates",
    "roleplayTemplates",
    "providerModels",
    "projects",
    "groups",
    "llmLogs",
    "pluginConfigs",
    "chatSettings",
    "folders",
    "wardrobeItems",
    "characterPluginData",
    "conversationAnnotations",
    "chatDocuments",
    "instanceSettings",
    "embeddingStatus",
    "conversationChunks",
    "tfidfVocabularies",
    "vectorIndexMetas",
    "vectorEntries",
    "docMountPoints",
    "docMountFolders",
    "docMountFiles",
    "docMountFileLinks",
    "docMountChunks",
    "docMountDocuments",
    "docMountBlobs",
    "projectDocMountLinks",
    "groupDocMountLinks",
    "groupCharacterMembers",
    "textReplacementRules",
];

fn corpus_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/uuid-remap-corpus.json")
}

/// The oracle's mocked `crypto.randomUUID`, exactly.
fn counting_remapper() -> UuidRemapper {
    let mut n = 0u64;
    UuidRemapper::with_id_source(Box::new(move || {
        n += 1;
        format!("00000000-0000-4000-8000-{n:012}")
    }))
}

fn rows(data: &Map<String, Value>, key: &str) -> Vec<Value> {
    match data.get(key) {
        Some(Value::Array(a)) => a.clone(),
        Some(other) => panic!("corpus collection `{key}` is not an array: {other}"),
        None => panic!("corpus case is missing collection `{key}`"),
    }
}

/// Build the core's [`BackupData`] out of one corpus case. Spelled out field by
/// field on purpose: the list doubles as the entity table's shape assertion.
fn backup_data(data: &Map<String, Value>) -> BackupData {
    BackupData {
        characters: rows(data, "characters"),
        chats: rows(data, "chats"),
        tags: rows(data, "tags"),
        connection_profiles: rows(data, "connectionProfiles"),
        image_profiles: rows(data, "imageProfiles"),
        embedding_profiles: rows(data, "embeddingProfiles"),
        memories: rows(data, "memories"),
        files: rows(data, "files"),
        prompt_templates: rows(data, "promptTemplates"),
        roleplay_templates: rows(data, "roleplayTemplates"),
        provider_models: rows(data, "providerModels"),
        projects: rows(data, "projects"),
        groups: rows(data, "groups"),
        llm_logs: rows(data, "llmLogs"),
        plugin_configs: rows(data, "pluginConfigs"),
        chat_settings: rows(data, "chatSettings"),
        folders: rows(data, "folders"),
        wardrobe_items: rows(data, "wardrobeItems"),
        character_plugin_data: rows(data, "characterPluginData"),
        conversation_annotations: rows(data, "conversationAnnotations"),
        chat_documents: rows(data, "chatDocuments"),
        instance_settings: rows(data, "instanceSettings"),
        embedding_status: rows(data, "embeddingStatus"),
        conversation_chunks: rows(data, "conversationChunks"),
        tfidf_vocabularies: rows(data, "tfidfVocabularies"),
        vector_index_metas: rows(data, "vectorIndexMetas"),
        vector_entries: rows(data, "vectorEntries"),
        doc_mount_points: rows(data, "docMountPoints"),
        doc_mount_folders: rows(data, "docMountFolders"),
        doc_mount_files: rows(data, "docMountFiles"),
        doc_mount_file_links: rows(data, "docMountFileLinks"),
        doc_mount_chunks: rows(data, "docMountChunks"),
        doc_mount_documents: rows(data, "docMountDocuments"),
        doc_mount_blobs: rows(data, "docMountBlobs"),
        project_doc_mount_links: rows(data, "projectDocMountLinks"),
        group_doc_mount_links: rows(data, "groupDocMountLinks"),
        group_character_members: rows(data, "groupCharacterMembers"),
        text_replacement_rules: rows(data, "textReplacementRules"),
    }
}

/// The produced collection, by v4's JSON name — so the diff can walk
/// [`COLLECTIONS`] in v4's order.
fn produced(out: &BackupData, key: &str) -> Vec<Value> {
    match key {
        "characters" => out.characters.clone(),
        "chats" => out.chats.clone(),
        "tags" => out.tags.clone(),
        "connectionProfiles" => out.connection_profiles.clone(),
        "imageProfiles" => out.image_profiles.clone(),
        "embeddingProfiles" => out.embedding_profiles.clone(),
        "memories" => out.memories.clone(),
        "files" => out.files.clone(),
        "promptTemplates" => out.prompt_templates.clone(),
        "roleplayTemplates" => out.roleplay_templates.clone(),
        "providerModels" => out.provider_models.clone(),
        "projects" => out.projects.clone(),
        "groups" => out.groups.clone(),
        "llmLogs" => out.llm_logs.clone(),
        "pluginConfigs" => out.plugin_configs.clone(),
        "chatSettings" => out.chat_settings.clone(),
        "folders" => out.folders.clone(),
        "wardrobeItems" => out.wardrobe_items.clone(),
        "characterPluginData" => out.character_plugin_data.clone(),
        "conversationAnnotations" => out.conversation_annotations.clone(),
        "chatDocuments" => out.chat_documents.clone(),
        "instanceSettings" => out.instance_settings.clone(),
        "embeddingStatus" => out.embedding_status.clone(),
        "conversationChunks" => out.conversation_chunks.clone(),
        "tfidfVocabularies" => out.tfidf_vocabularies.clone(),
        "vectorIndexMetas" => out.vector_index_metas.clone(),
        "vectorEntries" => out.vector_entries.clone(),
        "docMountPoints" => out.doc_mount_points.clone(),
        "docMountFolders" => out.doc_mount_folders.clone(),
        "docMountFiles" => out.doc_mount_files.clone(),
        "docMountFileLinks" => out.doc_mount_file_links.clone(),
        "docMountChunks" => out.doc_mount_chunks.clone(),
        "docMountDocuments" => out.doc_mount_documents.clone(),
        "docMountBlobs" => out.doc_mount_blobs.clone(),
        "projectDocMountLinks" => out.project_doc_mount_links.clone(),
        "groupDocMountLinks" => out.group_doc_mount_links.clone(),
        "groupCharacterMembers" => out.group_character_members.clone(),
        "textReplacementRules" => out.text_replacement_rules.clone(),
        other => panic!("unknown collection `{other}`"),
    }
}

/// Show where two serialized collections first diverge — a 100 KB `chats`
/// string is otherwise unreadable.
fn first_diff(a: &str, b: &str) -> String {
    let ab = a.as_bytes();
    let bb = b.as_bytes();
    let at = ab.iter().zip(bb).position(|(x, y)| x != y);
    match at {
        Some(i) => {
            let from = i.saturating_sub(120);
            format!(
                "first differs at byte {i}\n    rust:   …{}\n    oracle: …{}",
                &a[from..(i + 120).min(a.len())],
                &b[from..(i + 120).min(b.len())]
            )
        }
        None => format!(
            "common prefix identical; lengths {} vs {}",
            a.len(),
            b.len()
        ),
    }
}

#[test]
fn backup_uuid_remap_equivalence() {
    let Ok(path) = std::env::var("QT_ORACLE_UUID_REMAP") else {
        eprintln!("SKIP: QT_ORACLE_UUID_REMAP unset");
        return;
    };
    let raw = std::fs::read_to_string(&path).expect("read oracle ndjson");
    let oracle: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("oracle line is JSON"))
        .collect();
    assert!(!oracle.is_empty(), "oracle produced no cases");

    // The corpus, pinned by hash to the oracle run that produced the NDJSON.
    let corpus_bytes = std::fs::read(corpus_path()).expect("read committed corpus");
    let corpus_sha = hex::encode(Sha256::digest(&corpus_bytes));
    for line in &oracle {
        assert_eq!(
            line["corpusSha256"].as_str().unwrap(),
            corpus_sha,
            "the committed corpus is NOT the one the oracle ran against — \
             regenerate both together (see the oracle case's header recipe)"
        );
    }
    let corpus: Value = serde_json::from_slice(&corpus_bytes).expect("corpus is JSON");

    // Shape-rot guard: the corpus's own collection list must be exactly the
    // entity table this test walks (the `harness-corpus-shape-constants-rot`
    // rule — a hand-counted case total would not catch a lost collection).
    let meta_cols: Vec<&str> = corpus["_meta"]["collections"]
        .as_array()
        .expect("_meta.collections")
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(
        meta_cols,
        COLLECTIONS.to_vec(),
        "the corpus's collection list drifted from the entity table"
    );

    let cases = corpus["cases"].as_array().expect("corpus cases").clone();
    assert_eq!(
        cases.len(),
        oracle.len(),
        "corpus has {} cases, the oracle emitted {}",
        cases.len(),
        oracle.len()
    );

    // Tier-2 coverage: every collection carries a non-empty row SOMEWHERE.
    let uncovered: Vec<&str> = COLLECTIONS
        .iter()
        .copied()
        .filter(|c| {
            !cases.iter().any(|case| {
                case["data"][c]
                    .as_array()
                    .is_some_and(|rows| !rows.is_empty())
            })
        })
        .collect();
    assert!(
        uncovered.is_empty(),
        "the corpus exercises no row at all for: {uncovered:?}"
    );

    let mut failures: Vec<String> = Vec::new();
    for (case, expected) in cases.iter().zip(&oracle) {
        let name = case["name"].as_str().expect("case name");
        assert_eq!(
            name,
            expected["name"].as_str().unwrap(),
            "corpus and oracle case order diverged"
        );
        let target = case["targetUserId"].as_str().expect("targetUserId");
        let data = case["data"].as_object().expect("case data");

        let input = backup_data(data);
        let mut remapper = counting_remapper();
        let out = remap_backup_data(&input, target, &mut remapper);

        // 1. The oracle's returned object carries exactly the 38 collections.
        let want_obj = expected["output"].as_object().expect("oracle output");
        let want_keys: Vec<&str> = want_obj.keys().map(String::as_str).collect();
        if want_keys != COLLECTIONS.to_vec() {
            failures.push(format!(
                "[{name}] v4 returned a different key set/order: {want_keys:?}"
            ));
        }

        // 2. Every collection, byte-level.
        let mut diffs = 0usize;
        for key in COLLECTIONS {
            let got = serde_json::to_string(&Value::Array(produced(&out, key))).unwrap();
            let want = serde_json::to_string(&want_obj[key]).unwrap();
            if got != want {
                diffs += 1;
                failures.push(format!(
                    "[{name}] {key} differs\n  {}",
                    first_diff(&got, &want)
                ));
            }
        }

        // 3. The memo, entry for entry and in insertion order.
        let got_map = serde_json::to_string(&Value::Object(remapper.mapping_object())).unwrap();
        let want_map = serde_json::to_string(&expected["mapping"]).unwrap();
        if got_map != want_map {
            failures.push(format!(
                "[{name}] getMapping() differs\n  {}",
                first_diff(&got_map, &want_map)
            ));
        }

        // 4. getSize().
        let want_size = expected["size"].as_u64().unwrap() as usize;
        if remapper.size() != want_size {
            failures.push(format!(
                "[{name}] getSize(): rust {} vs oracle {want_size}",
                remapper.size()
            ));
        }

        if diffs == 0 {
            println!(
                "OK {name}: 38 collections byte-identical, {} ids remapped",
                remapper.size()
            );
        }
    }

    assert!(
        failures.is_empty(),
        "{} uuid-remap difference(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}
