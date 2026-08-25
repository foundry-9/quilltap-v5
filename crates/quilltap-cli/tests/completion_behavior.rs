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
