//! Behavioural guard for the shipped shell-completion templates — the v5-side
//! mirror of v4's `packages/quilltap/lib/__tests__/completion-behavior.test.js`
//! (added at `6afacb18` with bug 101, extended at `8f910137`; both vintages'
//! cases are carried here).
//!
//! The Tier R differential (`tests/cli_differential.rs`) proves our templates
//! are v4's bytes. This proves the bytes actually *do* something: completion
//! still fires once flags are on the line — the failure bug 101 shipped with,
//! where the verb was looked up by counting words rather than parsing them.
//!
//! Bash is driven for real (source the script, set `COMP_WORDS`/`COMP_CWORD`,
//! read `COMPREPLY` back) against a stub `quilltap` on `PATH`. Zsh's
//! completion system can only be driven from inside a completion widget, so
//! its template is checked structurally instead — plus a `zsh -n` parse where
//! a real zsh answers (v4 skips that one arm where the shell is absent; so do
//! we).
//!
//! The templates read here are the SHIPPED bytes: the same `include_str!`
//! paths `src/completion_cmd.rs` emits from, so a template that drifts from
//! what `quilltap completion bash` prints cannot exist.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BASH_TEMPLATE: &str = include_str!("../src/help/completion/bash.template");
const ZSH_TEMPLATE: &str = include_str!("../src/help/completion/zsh.template");

/// A `quilltap` on PATH that answers the completion lookups deterministically
/// (v4's `makeStubBin`). Two names carry a space on purpose: `compgen -W`
/// would chop them, which is why the templates route through
/// `_quilltap_lines_compreply`.
fn make_stub_bin(dir: &Path) {
    let stub = dir.join("quilltap");
    let body = concat!(
        "#!/bin/sh\n",
        "case \"$*\" in\n",
        "  *\"instances list --names-only\"*) printf \"StubInstance\\n\" ;;\n",
        "  *\"docs list --names-only\"*) printf \"Stub Store\\nOther Store\\n\" ;;\n",
        "esac\n",
        "exit 0\n",
    );
    std::fs::write(&stub, body).expect("write stub quilltap");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755))
            .expect("chmod stub quilltap");
    }
}

/// Whether a working shell of this name answers.
fn has_shell(shell: &str) -> bool {
    Command::new(shell)
        .args(["-c", ":"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

struct Bash {
    _root: tempfile::TempDir,
    stub_dir: PathBuf,
    template: PathBuf,
}

impl Bash {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let stub_dir = root.path().join("bin");
        std::fs::create_dir_all(&stub_dir).unwrap();
        make_stub_bin(&stub_dir);
        let template = root.path().join("bash.template");
        std::fs::write(&template, BASH_TEMPLATE).expect("write bash template");
        Bash {
            _root: root,
            stub_dir,
            template,
        }
    }

    /// Complete `line` with the shipped bash template and return the candidate
    /// list. A trailing space means "start a new word", exactly as at a real
    /// prompt (v4's `bashComplete`).
    fn complete(&self, line: &str) -> Vec<String> {
        let script = format!(
            r#"
    source {tpl}
    COMP_LINE={line}
    COMP_POINT=${{#COMP_LINE}}
    eval "COMP_WORDS=($COMP_LINE)"
    [[ "$COMP_LINE" =~ [[:space:]]$ ]] && COMP_WORDS+=("")
    COMP_CWORD=$(( ${{#COMP_WORDS[@]}} - 1 ))
    _quilltap_complete
    printf '%s\n' "${{COMPREPLY[@]}}"
"#,
            tpl = shell_quote(&self.template.to_string_lossy()),
            line = shell_quote(line),
        );
        let path = format!(
            "{}:{}",
            self.stub_dir.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let out = Command::new("bash")
            .args(["-c", &script])
            .env("PATH", path)
            .output()
            .expect("run bash");
        assert!(
            out.status.success(),
            "bash completion driver failed for {line:?}:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| l.to_string())
            .collect()
    }
}

/// Single-quote for the shell (the JSON.stringify v4 leans on has no Rust
/// twin; a POSIX single-quoted word is the exact equivalent here).
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r#"'\''"#))
}

fn assert_offers(got: &[String], want: &str, line: &str) {
    assert!(
        got.iter().any(|c| c == want),
        "`{line}` should offer {want:?}, got {got:?}"
    );
}

fn assert_withholds(got: &[String], unwanted: &str, line: &str) {
    assert!(
        !got.iter().any(|c| c == unwanted),
        "`{line}` should NOT offer {unwanted:?}, got {got:?}"
    );
}

#[test]
fn bash_completion_survives_flags_on_the_line() {
    if !has_shell("bash") {
        eprintln!(
            "SKIPPING bash_completion_survives_flags_on_the_line: no working `bash` on this host"
        );
        return;
    }
    let bash = Bash::new();

    // The baseline: no flags at all.
    let line = "quilltap docs ";
    assert_offers(&bash.complete(line), "list", line);

    // The headline: a flag on the line must not silence the verb list.
    for line in [
        "quilltap docs --instance Friday ",        // an instance flag
        "quilltap docs -i Friday ",                // a short instance flag
        "quilltap docs --limit 5 ",                // a subcommand flag with a value
        "quilltap docs --json ",                   // a valueless flag
        "quilltap --instance Friday docs --json ", // flags on both sides
    ] {
        assert_offers(&bash.complete(line), "list", line);
    }

    let line = "quilltap db --limit 5 ";
    assert_offers(&bash.complete(line), "characters", line);

    let line = "quilltap db characters --instance Friday ";
    assert_offers(&bash.complete(line), "status", line);

    // `-i` is `--ignore-case` under `memories`, not `--instance`: it takes no
    // value, so the word after it is still the verb — and no instance names.
    let line = "quilltap memories -i ";
    let got = bash.complete(line);
    assert_offers(&got, "ls", line);
    assert_withholds(&got, "StubInstance", line);
}

#[test]
fn bash_completion_looks_up_names_against_the_addressed_instance() {
    if !has_shell("bash") {
        eprintln!("SKIPPING bash_completion_looks_up_names_against_the_addressed_instance: no working `bash` on this host");
        return;
    }
    let bash = Bash::new();

    // Store names carry spaces; the candidates come back `printf '%q'`-escaped
    // rather than chopped into two.
    for line in [
        "quilltap docs --mount ",       // the flag's value
        "quilltap docs ls ",            // a store positional
        "quilltap docs move Src a.md ", // the DESTINATION store of a move
    ] {
        assert_offers(&bash.complete(line), r"Stub\ Store", line);
    }

    // `docs find` takes a query, not a store.
    let line = "quilltap docs find ";
    assert_withholds(&bash.complete(line), r"Stub\ Store", line);
}

/// v4's `/\(\(\s*CURRENT\s*==/` — an arithmetic test on the word index,
/// whatever the spacing.
fn has_word_index_test(template: &str) -> bool {
    template.match_indices("((").any(|(i, _)| {
        let rest = template[i + 2..].trim_start();
        rest.strip_prefix("CURRENT")
            .is_some_and(|r| r.trim_start().starts_with("=="))
    })
}

#[test]
fn zsh_completion_parses_positions_instead_of_counting_words() {
    // `(( CURRENT == 2 ))` is the bug: it only holds when the verb sits
    // immediately after the subcommand, so any preceding flag hides it. v4
    // greps `/\(\(\s*CURRENT\s*==/`, so the check tolerates the spacing.
    assert!(
        !has_word_index_test(ZSH_TEMPLATE),
        "the zsh template still tests a hard-coded word index"
    );

    // Without the `(-)` prefixes the rest-argument array comes back empty and
    // `_quilltap_subcommand` has nothing to dispatch on.
    assert!(
        ZSH_TEMPLATE.contains("'(-): :->subcommand'"),
        "the top-level subcommand positional lost its (-) prefix"
    );
    assert!(
        ZSH_TEMPLATE.contains("'(-)*::arg:->args'"),
        "the top-level rest argument lost its (-) prefix"
    );

    // Every subcommand hands its verb to `_arguments` as a positional. v4
    // counts `/'\(?-?\)?1: :->\w+'/g` matches; every occurrence of the
    // `1: :->` spec in the template is one of those, so counting the literal
    // is the same number (7 at `6afacb18`).
    let dispatchers = ZSH_TEMPLATE.matches("1: :->").count();
    assert!(
        dispatchers >= 6,
        "expected at least 6 `1: :->` positional dispatchers, found {dispatchers}"
    );
}

#[test]
fn zsh_template_is_syntactically_valid() {
    // Needs the shell itself; skipped rather than failed where it is absent
    // (v4 `8f910137` — GitHub's ubuntu runners ship without zsh).
    if !has_shell("zsh") {
        eprintln!("SKIPPING zsh_template_is_syntactically_valid: no working `zsh` on this host");
        return;
    }
    let root = tempfile::tempdir().expect("tempdir");
    let file = root.path().join("_quilltap");
    let mut f = std::fs::File::create(&file).unwrap();
    f.write_all(ZSH_TEMPLATE.as_bytes()).unwrap();
    drop(f);
    let out = Command::new("zsh")
        .arg("-n")
        .arg(&file)
        .output()
        .expect("run zsh -n");
    assert!(
        out.status.success(),
        "zsh -n rejected the shipped template:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// ---------------------------------------------------------------------------
// The flag-coverage guards — v5's mirror of v4's `completion-coverage.test.js`
// additions at `57e7b1bc2` (P4.D128).
//
// The arm-per-subcommand check v4 already had is still too coarse: `docs
// docker-mounts` had its own arm in all three shells while `--format`, its only
// flag, was offered by none of them. A flag documented in a subcommand's own
// `--help` is the contract the user reads, so that text is the source of truth
// here — whatever `--help` advertises, the three templates must offer.
//
// v4 reads its help out of the JS function that prints it; v5's help IS a file
// (`src/help/*_help.txt`, byte-copied from v4 and pinned by Tier R), except
// `recall-replay`, whose help is an inline `const HELP` extracted from source
// the same way v4 extracts a function body.
//
// v4's guard covers all twelve subcommands. v5 dispatches five of them and
// answers `not_yet_available` for the rest, so the map covers exactly the
// dispatched set — and `help_sources_cover_every_dispatched_subcommand` below
// makes that a mechanical fact, not a comment: implementing another subcommand
// fails this file until its help lands here too.
// ---------------------------------------------------------------------------

const FISH_TEMPLATE: &str = include_str!("../src/help/completion/fish.template");
const MAIN_RS: &str = include_str!("../src/main.rs");
const RECALL_REPLAY_RS: &str = include_str!("../src/recall_replay_cmd.rs");

const DB_HELP: &str = include_str!("../src/help/db_help.txt");
const DOCS_HELP: &str = include_str!("../src/help/docs_help.txt");
const INSTANCES_HELP: &str = include_str!("../src/help/instances_help.txt");
const COMPLETION_HELP: &str = include_str!("../src/help/completion_help.txt");

/// Each entry names the text one subcommand's `--help` prints.
fn help_sources() -> Vec<(&'static str, String)> {
    vec![
        ("db", DB_HELP.to_string()),
        ("docs", DOCS_HELP.to_string()),
        ("instances", INSTANCES_HELP.to_string()),
        ("completion", COMPLETION_HELP.to_string()),
        ("recall-replay", recall_replay_help()),
    ]
}

/// `recall-replay`'s help is an inline `const HELP: &str = "…";` — pulled from
/// the source, as v4 pulls its help out of a function body.
fn recall_replay_help() -> String {
    let start = RECALL_REPLAY_RS
        .find("const HELP: &str = \"")
        .expect("locate `const HELP` in recall_replay_cmd.rs")
        + "const HELP: &str = \"".len();
    let rest = &RECALL_REPLAY_RS[start..];
    let end = rest
        .find("\";")
        .expect("locate the end of `const HELP` in recall_replay_cmd.rs");
    rest[..end].to_string()
}

/// v4's `const SUBCOMMANDS: &[&str] = &[…]` twin in `src/main.rs` — parsed, so
/// a subcommand added to the dispatch table cannot slip past these guards.
fn subcommands() -> Vec<String> {
    const DECL: &str = "const SUBCOMMANDS: &[&str] = &[";
    let start = MAIN_RS.find(DECL).expect("locate SUBCOMMANDS in main.rs") + DECL.len();
    let rest = &MAIN_RS[start..];
    let close = rest.find("];").expect("locate the end of SUBCOMMANDS");
    rest[..close]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Long flags named anywhere in one subcommand's help text (v4's
/// `/--[a-z0-9][a-z0-9-]+/g`, deduped and sorted).
fn flags_in_help(help: &str) -> Vec<String> {
    let b = help.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i + 3 < b.len() {
        if b[i] == b'-'
            && b[i + 1] == b'-'
            && (b[i + 2].is_ascii_lowercase() || b[i + 2].is_ascii_digit())
        {
            let mut j = i + 3;
            while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'-') {
                j += 1;
            }
            // `--a` is two chars past the dashes at minimum: v4's pattern needs
            // one leading [a-z0-9] plus at least one more [a-z0-9-].
            if j >= i + 4 {
                let flag = help[i..j].to_string();
                if !out.contains(&flag) {
                    out.push(flag);
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    out.sort();
    out
}

/// `--max` is a prefix of `--max-nodes`, so a plain substring test passes for a
/// flag that is not actually there. Require the match to end at a non-flag
/// character (v4's `mentionsFlag`).
fn mentions_flag(haystack: &str, flag: &str) -> bool {
    let mut from = 0usize;
    while let Some(hit) = haystack[from..].find(flag) {
        let at = from + hit;
        let after = haystack[at + flag.len()..].chars().next();
        match after {
            Some(c) if c.is_ascii_alphanumeric() || c == '-' => {}
            _ => return true,
        }
        from = at + flag.len();
    }
    false
}

#[test]
fn help_sources_cover_every_dispatched_subcommand() {
    // A subcommand v5 dispatches for real (rather than answering
    // `not_yet_available`) needs a help source above, or its flags go
    // unchecked. v4's twin asserts the map covers ALL of SUBCOMMANDS; v5's
    // covers exactly the implemented slice, computed from the dispatch arms.
    let subs = subcommands();
    assert!(
        subs.len() >= 10,
        "expected the full v4 subcommand surface, parsed {subs:?}"
    );
    let dispatched: Vec<String> = subs
        .iter()
        .filter(|s| MAIN_RS.contains(&format!("\"{s}\" => ")))
        .cloned()
        .collect();
    let mut covered: Vec<String> = help_sources()
        .iter()
        .map(|(s, _)| (*s).to_string())
        .collect();
    let mut expected = dispatched.clone();
    covered.sort();
    expected.sort();
    assert_eq!(
        covered, expected,
        "the help-source map and the dispatch table disagree"
    );
}

#[test]
fn completions_offer_every_flag_the_help_text_advertises() {
    let mut failures: Vec<String> = Vec::new();
    for (sub, help) in help_sources() {
        let flags = flags_in_help(&help);
        assert!(
            !flags.is_empty(),
            "`{sub}` help text yielded no long flags — the extractor is broken"
        );
        for (shell, tpl) in [
            ("bash", BASH_TEMPLATE),
            ("zsh", ZSH_TEMPLATE),
            ("fish", FISH_TEMPLATE),
        ] {
            let missing: Vec<&String> = flags
                .iter()
                .filter(|flag| {
                    // fish spells the flag `-l 'name'`, already an exact
                    // quoted token.
                    if shell == "fish" {
                        !tpl.contains(&format!("-l '{}'", &flag[2..]))
                    } else {
                        !mentions_flag(tpl, flag)
                    }
                })
                .collect();
            if !missing.is_empty() {
                failures.push(format!("{sub}: {shell} template is missing {missing:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// v4 bug 120 (`af2023c9a`) also closed the completion half: `instances --help`
/// names `--json` on the `list` line, and the FISH template — alone of the
/// three — offered it on no instances verb at all. bash's `inst_flags` and
/// zsh's `inst_opts` already carried it (verified against v4's own templates at
/// the pin: the hunk touches `fish.template` only), so the offer is scoped to
/// the two `list` spellings exactly as v4 spells it. The blanket flag-coverage
/// test above cannot see this: it asks only whether `-l 'json'` appears
/// ANYWHERE in the template, and it did — on the top-level `quilltap --json`.
#[test]
fn fish_offers_json_on_both_instances_list_spellings() {
    assert!(
        FISH_TEMPLATE.contains(
            "for verb in list ls
  complete -c quilltap -n \"__quilltap_using_subverb instances $verb\" -l 'json' -d 'JSON output'
end
"
        ),
        "fish template is missing v4's `instances list|ls --json` block"
    );
    // v4 places it between the `--names-only` line and the `default --clear`
    // line; a block in the wrong section would still satisfy the containment
    // check above, so the neighbours are pinned too.
    let names_only = FISH_TEMPLATE
        .find("-l 'names-only' -d 'Print one name per line'")
        .expect("locate the instances --names-only offer");
    let json_block = FISH_TEMPLATE
        .find("for verb in list ls")
        .expect("locate the instances list --json block");
    let clear = FISH_TEMPLATE
        .find("instances default' -l 'clear'")
        .expect("locate the instances default --clear offer");
    assert!(names_only < json_block && json_block < clear);
}

/// bash cannot infer which flags swallow the next word, so it carries explicit
/// `vf_*` lists. A valued flag missing from its list makes the flag's value look
/// like the subcommand's verb — the bug 101 failure mode. zsh and fish take the
/// value from the flag's own spec, so only bash needs guarding.
#[test]
fn bash_knows_which_docs_flags_take_a_value() {
    // The scanner reads `$vf_global$vf_docs`, so a flag in either list counts.
    let scanned: Vec<String> = [
        shell_local_list(BASH_TEMPLATE, "vf_global"),
        shell_local_list(BASH_TEMPLATE, "vf_docs"),
    ]
    .concat();
    // A docs flag zsh declares with a `:value:` spec is by definition valued.
    let valued = zsh_valued_docs_flags(ZSH_TEMPLATE);
    assert!(
        valued.len() > 5,
        "expected zsh's docs_opts to declare more than five valued flags, found {valued:?}"
    );
    let missing: Vec<&String> = valued.iter().filter(|f| !scanned.contains(f)).collect();
    assert!(
        missing.is_empty(),
        "bash's vf_global/vf_docs do not list the valued docs flags {missing:?}"
    );
}

/// `local <name>=" a b c "` → its whitespace-delimited tokens.
fn shell_local_list(template: &str, name: &str) -> Vec<String> {
    let needle = format!("local {name}=\"");
    let start = template
        .find(&needle)
        .unwrap_or_else(|| panic!("locate `local {name}=` in the bash template"))
        + needle.len();
    let rest = &template[start..];
    let end = rest
        .find('"')
        .unwrap_or_else(|| panic!("locate the end of `local {name}=`"));
    rest[..end]
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// The `'--flag[description]:value:…'` entries of zsh's `docs_opts=( … )`
/// (v4's `/'(--[a-z0-9-]+)\[[^\]]*\]:[^']*'/g`). Entries that open with a
/// `'(-x --long)'` exclusion group are not in this shape and are skipped, as
/// they are in v4.
fn zsh_valued_docs_flags(template: &str) -> Vec<String> {
    let start = template
        .find("docs_opts=(")
        .expect("locate docs_opts in the zsh template");
    let rest = &template[start..];
    let end = rest.find("\n  )").expect("locate the end of docs_opts");
    let mut out = Vec::new();
    for line in rest[..end].lines() {
        let line = line.trim();
        let Some(body) = line.strip_prefix("'--") else {
            continue;
        };
        let Some(bracket) = body.find('[') else {
            continue;
        };
        let Some(close) = body.find(']') else {
            continue;
        };
        if close < bracket {
            continue;
        }
        // A valued spec is `]:` — a boolean flag closes with `]'`.
        if body[close + 1..].starts_with(':') {
            out.push(format!("--{}", &body[..bracket]));
        }
    }
    out
}
