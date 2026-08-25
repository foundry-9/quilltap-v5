//! `quilltap completion <shell>` — the shell-completion emitters (v4
//! `packages/quilltap/lib/completion-commands.js`). The three scripts are
//! byte transcriptions of v4's `lib/completion/*.template` files (re-captured
//! at v4 `6afacb18` — bug 101, the flag-tolerant rewrite of all three; before
//! that at `03154b72`, the `db characters` sub-subverb family), emitted
//! verbatim. v4's emitter reads the template and writes it through untouched,
//! so there is nothing to substitute on either side — the shipped bytes ARE
//! v4's file. `tests/completion_behavior.rs` drives these same bytes under a
//! real bash/zsh; `tests/cli_differential.rs` byte-diffs them against v4's
//! launcher.

use crate::out;

const COMPLETION_HELP: &str = include_str!("help/completion_help.txt");
const BASH_TEMPLATE: &str = include_str!("help/completion/bash.template");
const ZSH_TEMPLATE: &str = include_str!("help/completion/zsh.template");
const FISH_TEMPLATE: &str = include_str!("help/completion/fish.template");

pub fn run(args: &[String]) {
    // v4: no args → help + exit 1; a leading -h/--help → help + exit 0.
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        out::write_stdout(COMPLETION_HELP.as_bytes());
        out::exit(if args.is_empty() { 1 } else { 0 });
    }
    let shell = args[0].as_str();
    let template = match shell {
        "bash" => BASH_TEMPLATE,
        "zsh" => ZSH_TEMPLATE,
        "fish" => FISH_TEMPLATE,
        _ => {
            out::elog(&format!(
                "Error: Unknown shell '{shell}'. Supported shells: bash, zsh, fish"
            ));
            out::exit(1);
        }
    };
    out::write_stdout(template.as_bytes());
}
