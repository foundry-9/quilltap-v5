//! Tier-2 differential (P4.4 unit 2, sub-unit 3): the identity-stack compiler
//! (`services::system_prompt_compiler::compile_all_identity_stacks`) vs v4's REAL
//! `compileAllIdentityStacks`. Both sides read a COPY of the SAME baked fixture (a
//! chat with four participants — Aria/llm, Bob/llm, Sam/user, Ghost/llm-removed —
//! and a `scenarioText`), run the compiler, and compare the persisted
//! `chats.compiledIdentityStacks` map EXACTLY (all ids pinned — zero
//! normalization; the compile mints no clock/id and does not bump `updatedAt`).
//!
//! Banks: only the two active LLM participants get a stack (Sam is user-controlled
//! → skipped; Ghost is removed → skipped); the chat-level `{{user}}`/`{{persona}}`/
//! `{{scenario}}` variables resolve into each stack; a `physicalDescription` on
//! Aria surfaces.
//!
//! Build the fixtures + oracle (Node 24, from the v4 checkout):
//!   N=~/.nvm/versions/node/v24.13.1/bin ; V5=~/source/quilltap-v5
//!   cd ~/source/quilltap-server
//!   QT_FIXTURE_IDC_MAIN=/tmp/qt-idc-main.db QT_FIXTURE_IDC_MOUNT=/tmp/qt-idc-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/fixtures/build-identity-compiler-fixture.ts
//!   QT_FIXTURE_IDC_MAIN=/tmp/qt-idc-main.db QT_FIXTURE_IDC_MOUNT=/tmp/qt-idc-mount.db \
//!     $N/node --import tsx $V5/harness/oracle/cases/identity-compiler.ts > /tmp/oracle-idc.ndjson
//! Run:
//!   QT_ORACLE_IDC=/tmp/oracle-idc.ndjson \
//!   QT_FIXTURE_IDC_MAIN=/tmp/qt-idc-main.db QT_FIXTURE_IDC_MOUNT=/tmp/qt-idc-mount.db \
//!     cargo test -p quilltap-harness --test identity_compiler_equivalence

use std::path::{Path, PathBuf};

use quilltap_core::db::chats_read;
use quilltap_core::db::Writer;
use quilltap_core::services::system_prompt_compiler::compile_all_identity_stacks;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Spec {
    #[serde(rename = "testPepperBase64")]
    test_pepper_base64: String,
    #[serde(rename = "chatId")]
    chat_id: String,
}

fn spec_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../harness/oracle/fixtures/identity-compiler.json")
}

fn env_or_skip(key: &str) -> Option<String> {
    match std::env::var(key) {
        Ok(v) => Some(v),
        Err(_) => {
            eprintln!("SKIP: set {key} (see test header).");
            None
        }
    }
}

#[test]
fn identity_compiler_matches_oracle() {
    let (Some(oracle_path), Some(main_fixture), Some(mount_fixture)) = (
        env_or_skip("QT_ORACLE_IDC"),
        env_or_skip("QT_FIXTURE_IDC_MAIN"),
        env_or_skip("QT_FIXTURE_IDC_MOUNT"),
    ) else {
        return;
    };

    let spec: Spec = serde_json::from_str(
        &std::fs::read_to_string(spec_path()).unwrap_or_else(|e| panic!("read spec: {e}")),
    )
    .expect("parse spec");

    let oracle_line =
        std::fs::read_to_string(&oracle_path).unwrap_or_else(|e| panic!("read oracle: {e}"));
    let oracle: Value = serde_json::from_str(oracle_line.trim()).expect("oracle line parses");
    let want = &oracle["compiledIdentityStacks"];

    let pid = std::process::id();
    let main_work = std::env::temp_dir().join(format!("qt-idc-main-rust-{pid}.db"));
    let mount_work = std::env::temp_dir().join(format!("qt-idc-mount-rust-{pid}.db"));
    let _ = std::fs::remove_file(&main_work);
    let _ = std::fs::remove_file(&mount_work);
    std::fs::copy(&main_fixture, &main_work).unwrap_or_else(|e| panic!("copy main: {e}"));
    std::fs::copy(&mount_fixture, &mount_work).unwrap_or_else(|e| panic!("copy mount: {e}"));

    let main_w = Writer::open_writable(&main_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open main: {e}"));
    let mount_w = Writer::open_writable(&mount_work, &spec.test_pepper_base64)
        .unwrap_or_else(|e| panic!("open mount: {e}"));
    let main = main_w.connection();
    let mount = mount_w.connection();

    let chat = chats_read::find_by_id(main, &spec.chat_id)
        .expect("read chat")
        .expect("chat present");

    compile_all_identity_stacks(main, mount, &chat).expect("compile");

    let after = chats_read::find_by_id(main, &spec.chat_id)
        .expect("re-read chat")
        .expect("chat present");
    // v4 hydrates the column to an object; a null column reads as absent → null.
    let got = after
        .get("compiledIdentityStacks")
        .cloned()
        .unwrap_or(Value::Null);

    assert_eq!(&got, want, "compiledIdentityStacks: rust != oracle");
}
