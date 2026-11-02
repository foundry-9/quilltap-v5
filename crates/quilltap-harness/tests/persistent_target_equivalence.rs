//! Tier-1 differential (P4.D354, v4 `faeae2b71` — contract C7): the
//! persistent-target check — `services::mount_index::persistent_target` vs
//! v4's REAL `lib/mount-index/persistent-target.ts` (`parseMountInfo`,
//! `findContainingMount`, `checkPersistentTarget` with the table injected and
//! `DOCKER_CONTAINER=true` in the case's env, so v4's server probe answers
//! true). v5 passes `is_docker: true` — the caller's input by design (R-4).
//!
//! Rows: `parse` (the mount entries of four tables — v4's own
//! `DOCKER_DESKTOP_MOUNTINFO`, an edge table with a `\040` / `\011` / `\134`
//! escape, optional fields, a tie, a `-` field 0 and malformed lines, the
//! malformed-only text, the empty text), `find` (14 paths per table), `check`
//! (11 targets per table, an unreadable table, and a symlinked directory whose
//! link "lives on the bind" but points into the overlay — rebuilt here at the
//! paths the oracle row names, then removed).
//!
//! Generate the oracle output:
//!   cd ~/source/quilltap-server
//!   npx tsx ~/source/quilltap-v5/harness/oracle/cases/persistent-target.ts \
//!     > /tmp/oracle-persistent-target.ndjson
//! Run:
//!   QT_ORACLE_PERSISTENT_TARGET=/tmp/oracle-persistent-target.ndjson \
//!     cargo test -p quilltap-harness --test persistent_target_equivalence

use quilltap_core::services::mount_index::persistent_target::{
    check_persistent_target, find_containing_mount, parse_mount_info, MountEntry, MountInfoSource,
    PersistentTargetCheck,
};
use serde_json::{json, Value};

/// The oracle's four named tables (v4's fixture + the edge cases), rebuilt
/// from the `parse` rows' SOURCE texts — identical to the case's constants.
fn texts() -> Vec<(&'static str, String)> {
    let docker_desktop = [
        "134 94 0:53 / / rw,relatime - overlay overlay rw,lowerdir=/x,upperdir=/y,workdir=/z",
        "136 134 0:62 / /proc rw,nosuid,nodev,noexec,relatime - proc proc rw",
        "137 134 0:63 / /dev rw,nosuid - tmpfs tmpfs rw,size=65536k,mode=755",
        "142 137 0:66 / /dev/shm rw,nosuid,nodev,noexec,relatime - tmpfs shm rw,size=65536k",
        "143 134 0:43 /csebold/Library/Mobile\\040Documents/Quilltap/V4test /app/quilltap rw,nosuid,nodev,relatime - virtiofs virtiofs0 rw",
        "150 134 0:43 /csebold/Vaults/My\\040Notes /Users/csebold/Vaults/My\\040Notes rw,relatime - virtiofs virtiofs0 rw",
        "144 134 254:1 /docker/containers/abc/hostname /etc/hostname rw,relatime - ext4 /dev/vda1 rw,discard",
        "160 134 0:70 / /scratch rw,relatime - tmpfs tmpfs rw",
        "170 134 254:1 /docker/srv/qtvol/_data /srv/qtvol rw,relatime shared:1 - ext4 /dev/vda1 rw",
        "",
    ]
    .join("\n");
    let edge = [
        "1 0 0:1 / / rw - overlay overlay rw",
        "2 1 0:2 / /data rw shared:2 master:3 - ext4 /dev/sda1 rw",
        "3 1 0:3 / /data rw - tmpfs tmpfs rw",
        "4 1 0:4 / /data/ rw - cgroup2 cgroup2 rw",
        "5 1 0:5 / /odd\\011tab\\134slash rw - virtiofs v rw",
        "6 1 0:6 / /mq rw - mqueue mqueue rw",
        "garbage",
        "7 1 0:7 / /nofs rw -",
        "8 1 - / /dashfirst rw - ext4 x rw",
        "9 1",
        "   ",
        "10 1 0:9 / /sys rw - sysfs sysfs rw",
        "11 1 0:10 / /vol rw - xfs /dev/x rw",
    ]
    .join("\n");
    vec![
        ("docker_desktop", docker_desktop),
        ("edge", edge),
        ("malformed", "garbage\n\n1 2 3".to_string()),
        ("empty", String::new()),
    ]
}

struct Text(String);
impl MountInfoSource for Text {
    fn read_mount_info(&self) -> std::io::Result<String> {
        Ok(self.0.clone())
    }
}
struct Unreadable;
impl MountInfoSource for Unreadable {
    fn read_mount_info(&self) -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "EACCES",
        ))
    }
}

fn entry_json(m: &MountEntry) -> Value {
    json!({ "mountPoint": m.mount_point, "fsType": m.fs_type })
}

fn check_json(c: &PersistentTargetCheck) -> Value {
    match c {
        PersistentTargetCheck::Persistent => json!({ "persistent": true }),
        PersistentTargetCheck::Ephemeral {
            mount_point,
            fs_type,
            message,
        } => json!({
            "persistent": false,
            "mountPoint": mount_point,
            "fsType": fs_type,
            "message": message,
        }),
    }
}

#[test]
fn persistent_target_matches_oracle() {
    let Ok(path) = std::env::var("QT_ORACLE_PERSISTENT_TARGET") else {
        eprintln!("SKIP: QT_ORACLE_PERSISTENT_TARGET not set");
        return;
    };
    let rows: Vec<Value> = std::fs::read_to_string(&path)
        .expect("read oracle")
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("parse row"))
        .collect();
    let texts = texts();
    let text_of = |name: &str| {
        texts
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, t)| t.clone())
    };
    let mut red = Vec::new();
    let (mut parse, mut find, mut check) = (0, 0, 0);
    for row in &rows {
        let kind = row["kind"].as_str().unwrap();
        let text_name = row["text"].as_str().unwrap();
        match kind {
            "parse" => {
                parse += 1;
                let got: Vec<Value> = parse_mount_info(&text_of(text_name).unwrap())
                    .iter()
                    .map(entry_json)
                    .collect();
                if Value::Array(got.clone()) != row["mounts"] {
                    red.push(format!("parse/{text_name}: {got:?} vs {}", row["mounts"]));
                }
            }
            "find" => {
                find += 1;
                let mounts = parse_mount_info(&text_of(text_name).unwrap());
                let p = row["path"].as_str().unwrap();
                let got = find_containing_mount(&mounts, p)
                    .map(entry_json)
                    .unwrap_or(Value::Null);
                if got != row["mount"] {
                    red.push(format!("find/{text_name}/{p:?}: {got} vs {}", row["mount"]));
                }
            }
            "check" => {
                check += 1;
                let target = row["target"].as_str().unwrap();
                let got = match text_name {
                    "unreadable" => check_persistent_target(target, true, &Unreadable),
                    "symlink" => {
                        // Rebuild the oracle's directory + link at its paths.
                        let setup = &row["setup"];
                        let dir = setup["dir"].as_str().unwrap();
                        let real = setup["target"].as_str().unwrap();
                        let link = setup["link"].as_str().unwrap();
                        let _ = std::fs::remove_dir_all(dir);
                        std::fs::create_dir_all(real).unwrap();
                        std::os::unix::fs::symlink(real, link).unwrap();
                        let got = check_persistent_target(
                            target,
                            true,
                            &Text(setup["mountinfo"].as_str().unwrap().to_string()),
                        );
                        std::fs::remove_dir_all(dir).unwrap();
                        got
                    }
                    name => check_persistent_target(target, true, &Text(text_of(name).unwrap())),
                };
                let got = check_json(&got);
                if got != row["check"] {
                    red.push(format!(
                        "check/{text_name}/{target:?}: {got} vs {}",
                        row["check"]
                    ));
                }
            }
            other => panic!("unknown row kind {other}"),
        }
    }
    assert!(
        red.is_empty(),
        "{} row(s) red:\n{}",
        red.len(),
        red.join("\n")
    );
    assert_eq!(
        (parse, find, check),
        (4, 4 * 14, 4 * 11 + 2),
        "the corpus lost rows"
    );
    eprintln!(
        "OK: persistent-target matched oracle ({parse} parse, {find} find, {check} check rows)."
    );
}
