//! P4.D145 (v4 bug 114) — the READ-ONLY folder-population measurement.
//!
//! Answers the work order's §10 question on a COPY of a real instance: how many
//! `folders` rows describe how many distinct `(userId, COALESCE(projectId,''),
//! path)` identities, and is the bug-114 unique index already there? Opens
//! read-only and writes nothing.
//!
//!   cargo run -p quilltap-harness --example p4d145_folder_population -- ~/qt-dogfood-friday
fn main() {
    let dir = std::env::args().nth(1).expect("usage: … -- <instance dir>");
    let data = std::path::Path::new(&dir).join("data");
    let pepper = quilltap_core::dbkey::load_pepper(&data, None).expect("unwrap .dbkey");
    let db = quilltap_core::db::runtime::Db::open(
        quilltap_core::db::runtime::DbPaths {
            main: data.join("quilltap.db"),
            mount_index: None,
            llm_logs: None,
        },
        &pepper,
    )
    .expect("open main read-only");

    let out = db
        .read_main(|c| {
            let total: i64 = c.query_row("SELECT COUNT(*) FROM folders", [], |r| r.get(0))?;
            let groups: i64 = c.query_row(
                "SELECT COUNT(*) FROM (SELECT 1 FROM folders \
                 GROUP BY userId, COALESCE(projectId, ''), path)",
                [],
                |r| r.get(0),
            )?;
            let indexed = quilltap_core::db::folders_unique_path_repair::index_exists(c)?;
            let mut stmt = c.prepare(
                "SELECT COALESCE(projectId,''), path, COUNT(*) AS n FROM folders \
                 GROUP BY userId, COALESCE(projectId,''), path HAVING n > 1 \
                 ORDER BY n DESC LIMIT 10",
            )?;
            let worst: Vec<(String, String, i64)> = stmt
                .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<Result<_, _>>()?;
            Ok::<_, quilltap_core::db::DbError>((total, groups, indexed, worst))
        })
        .expect("read");

    let (total, groups, indexed, worst) = out;
    println!("folders rows        : {total}");
    println!("distinct identities : {groups}");
    println!("duplicate rows      : {}", total - groups);
    println!("bug-114 index present: {indexed}");
    if worst.is_empty() {
        println!("no duplicated identities");
    } else {
        println!("worst offenders (project, path, rows):");
        for (p, path, n) in worst {
            println!(
                "  {:<38} {:<28} {n}",
                if p.is_empty() { "<general>" } else { &p },
                path
            );
        }
    }
}
