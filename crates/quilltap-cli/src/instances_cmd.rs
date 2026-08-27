//! `quilltap instances` — the registry CRUD (v4
//! `packages/quilltap/lib/instances-commands.js`), over the write verbs on
//! `quilltap-host::instances`.

use serde_json::{Map, Value};

use quilltap_host::instances::{expand_path, verify_passphrase, InstanceRegistry, PassphraseCheck};

use crate::nodefmt::{json_stringify_pretty, node_join};
use crate::out;
use crate::resolve::{prompt_line, prompt_passphrase};

const INSTANCES_HELP: &str = include_str!("help/instances_help.txt");

pub fn run(args: &[String]) {
    if args.is_empty() {
        if let Err(e) = cmd_list(false, false) {
            out::elog(&format!("Error: {e}"));
            out::exit(1);
        }
        return;
    }
    let verb = args[0].as_str();
    let rest: Vec<String> = args[1..].to_vec();

    let dispatch = || match verb {
        "-h" | "--help" | "help" => {
            out::write_stdout(INSTANCES_HELP.as_bytes());
            Ok(())
        }
        "list" | "ls" => {
            let names_only = rest.iter().any(|a| a == "--names-only");
            let json = rest.iter().any(|a| a == "--json");
            cmd_list(names_only, json)
        }
        "show" => cmd_show(rest.first().map(String::as_str)),
        "path" | "where" => {
            out::log(
                &InstanceRegistry::at_default_location()
                    .path()
                    .to_string_lossy(),
            );
            Ok(())
        }
        "add" | "create" => cmd_add(&rest),
        "remove" | "rm" | "delete" => cmd_remove(&rest),
        "set-passphrase" | "passphrase" => cmd_set_passphrase(&rest),
        "default" => cmd_default(&rest),
        "rename" => cmd_rename(&rest),
        "restore-key" | "rebuild-key" => crate::restore_key::cmd_restore_key(&rest),
        _ => {
            out::elog(&format!("Unknown instances verb: {verb}"));
            out::elog("Run \"quilltap instances --help\" for usage.");
            out::exit(1);
        }
    };
    let result: Result<(), String> = dispatch();

    if let Err(e) = result {
        out::elog(&format!("Error: {e}"));
        out::exit(1);
    }
}

/// v4 `formatRow`.
fn format_row(name: &str, instance_path: &str, has_passphrase: bool, is_default: bool) -> String {
    let tag = if has_passphrase {
        "[passphrase set]"
    } else {
        "[no passphrase]"
    };
    let marker = if is_default { "*" } else { " " };
    format!("{marker} {name:<20}  {tag:<18}  {instance_path}")
}

fn cmd_list(names_only: bool, json: bool) -> Result<(), String> {
    let registry = InstanceRegistry::at_default_location();
    let entries = registry.list_instances()?;
    if names_only {
        for entry in &entries {
            out::log(&entry.name);
        }
        return Ok(());
    }
    if json {
        let rows: Vec<Value> = entries
            .iter()
            .map(|e| {
                let mut m = Map::new();
                m.insert("name".into(), Value::from(e.name.clone()));
                m.insert("path".into(), Value::from(e.path.clone()));
                m.insert("expandedPath".into(), Value::from(e.expanded_path.clone()));
                m.insert("hasPassphrase".into(), Value::from(e.has_passphrase));
                m.insert("isDefault".into(), Value::from(e.is_default));
                Value::Object(m)
            })
            .collect();
        out::log(&json_stringify_pretty(&Value::from(rows)));
        return Ok(());
    }
    out::log(&format!(
        "Instances file: {}",
        registry.path().to_string_lossy()
    ));
    if entries.is_empty() {
        out::log("No instances registered. Add one with `quilltap instances add <name>`.");
        return Ok(());
    }
    out::log("");
    out::log(&format!("* {:<20}  {:<18}  PATH", "NAME", "PASSPHRASE"));
    for entry in &entries {
        out::log(&format_row(
            &entry.name,
            &entry.path,
            entry.has_passphrase,
            entry.is_default,
        ));
    }
    Ok(())
}

fn cmd_show(name: Option<&str>) -> Result<(), String> {
    let Some(name) = name.filter(|n| !n.is_empty()) else {
        out::elog("Usage: quilltap instances show <name>");
        out::exit(1);
    };
    let registry = InstanceRegistry::at_default_location();
    let inst = registry.resolve_instance(name)?;
    out::log(&format!("Name:        {}", inst.name));
    out::log(&format!("Path:        {}", inst.path));
    out::log(&format!(
        "Passphrase:  {}",
        if inst.passphrase.is_empty() {
            "not set"
        } else {
            "set"
        }
    ));

    let data_dir = node_join(&inst.path, "data");
    let dbkey = node_join(&data_dir, "quilltap.dbkey");
    let main_db = node_join(&data_dir, "quilltap.db");
    out::log(&format!(
        "Data dir:    {}{}",
        data_dir,
        if std::path::Path::new(&data_dir).exists() {
            ""
        } else {
            " (missing)"
        }
    ));
    out::log(&format!(
        ".dbkey:      {}",
        if std::path::Path::new(&dbkey).exists() {
            "present"
        } else {
            "missing"
        }
    ));
    out::log(&format!(
        "quilltap.db: {}",
        if std::path::Path::new(&main_db).exists() {
            "present"
        } else {
            "missing"
        }
    ));
    Ok(())
}

/// v4 `promptHiddenWithConfirm`.
fn prompt_hidden_with_confirm(label: &str) -> Result<String, String> {
    let first = prompt_passphrase(&format!("{label}: "))?;
    if first.is_empty() {
        return Ok(String::new());
    }
    let second = prompt_passphrase(&format!("{label} (confirm): "))?;
    if first != second {
        return Err("Passphrases did not match.".to_string());
    }
    Ok(first)
}

fn cmd_add(args: &[String]) -> Result<(), String> {
    let mut name = args.first().cloned().unwrap_or_default();
    let mut instance_path = args.get(1).cloned().unwrap_or_default();

    if name.is_empty() {
        name = prompt_line("Instance name: ").trim().to_string();
        if name.is_empty() {
            out::elog("Aborted: no name provided.");
            out::exit(1);
        }
    }
    if instance_path.is_empty() {
        instance_path = prompt_line(&format!(
            "Path for \"{name}\" (instance root, contains data/files/logs): "
        ))
        .trim()
        .to_string();
        if instance_path.is_empty() {
            out::elog("Aborted: no path provided.");
            out::exit(1);
        }
    }

    let expanded = expand_path(&instance_path);
    if !std::path::Path::new(&expanded).exists() {
        let proceed = prompt_line(&format!(
            "Path \"{expanded}\" does not exist. Save anyway? [y/N] "
        ))
        .trim()
        .to_lowercase();
        if proceed != "y" && proceed != "yes" {
            out::elog("Aborted.");
            out::exit(1);
        }
    }

    let want_pass = prompt_line("Record a passphrase for this instance? [y/N] ")
        .trim()
        .to_lowercase();
    let mut passphrase = String::new();
    if want_pass == "y" || want_pass == "yes" {
        passphrase = prompt_hidden_with_confirm("Passphrase")?;
        if passphrase.is_empty() {
            out::log("Empty passphrase — not recording one.");
        } else {
            match verify_passphrase(&expanded, &passphrase)? {
                PassphraseCheck::Valid => {
                    out::log("Passphrase verified against .dbkey.");
                }
                PassphraseCheck::Wrong => {
                    out::elog("Passphrase does not unlock this instance's .dbkey. Not saving.");
                    out::exit(1);
                }
                PassphraseCheck::NoEncryption => {
                    out::elog("This instance's .dbkey does not require a passphrase. Not saving.");
                    out::exit(1);
                }
                PassphraseCheck::NoDbkey => {
                    out::log(
                        "No .dbkey on disk yet — passphrase will be saved without verification.",
                    );
                }
            }
        }
    }

    let registry = InstanceRegistry::at_default_location();
    let key = registry.upsert_instance(&name, &instance_path, &passphrase)?;
    out::log(&format!(
        "Saved instance \"{key}\" → {}",
        registry.path().to_string_lossy()
    ));
    Ok(())
}

fn cmd_remove(args: &[String]) -> Result<(), String> {
    let Some(name) = args.first().filter(|n| !n.is_empty()) else {
        out::elog("Usage: quilltap instances remove <name>");
        out::exit(1);
    };
    let key = InstanceRegistry::at_default_location().remove_instance(name)?;
    out::log(&format!("Removed instance \"{key}\"."));
    Ok(())
}

fn cmd_set_passphrase(args: &[String]) -> Result<(), String> {
    let Some(name) = args.first().filter(|n| !n.is_empty()) else {
        out::elog("Usage: quilltap instances set-passphrase <name>");
        out::exit(1);
    };
    let registry = InstanceRegistry::at_default_location();
    let inst = registry.resolve_instance(name)?;

    let want_clear = prompt_line("Clear stored passphrase? [y/N] ")
        .trim()
        .to_lowercase();
    if want_clear == "y" || want_clear == "yes" {
        registry.set_instance_passphrase(&inst.name, "")?;
        out::log(&format!("Cleared passphrase for \"{}\".", inst.name));
        return Ok(());
    }

    let passphrase = prompt_hidden_with_confirm("New passphrase")?;
    if passphrase.is_empty() {
        out::log("Empty passphrase — not changing the existing entry. Use the \"clear\" prompt above to remove it.");
        return Ok(());
    }

    match verify_passphrase(&inst.path, &passphrase)? {
        PassphraseCheck::Valid => {
            out::log("Passphrase verified against .dbkey.");
        }
        PassphraseCheck::Wrong => {
            out::elog("Passphrase does not unlock this instance's .dbkey. Not saving.");
            out::exit(1);
        }
        PassphraseCheck::NoEncryption => {
            out::elog("This instance's .dbkey does not require a passphrase. Not saving.");
            out::exit(1);
        }
        PassphraseCheck::NoDbkey => {
            out::log("No .dbkey on disk yet — passphrase will be saved without verification.");
        }
    }

    registry.set_instance_passphrase(&inst.name, &passphrase)?;
    out::log(&format!("Updated passphrase for \"{}\".", inst.name));
    Ok(())
}

fn cmd_default(args: &[String]) -> Result<(), String> {
    let registry = InstanceRegistry::at_default_location();
    if args.is_empty() {
        match registry.get_default_instance()? {
            Some(current) => out::log(&current),
            None => out::log("(none)"),
        }
        return Ok(());
    }
    let name = &args[0];
    if name == "--clear" {
        registry.clear_default_instance()?;
        out::log("Cleared default instance.");
        return Ok(());
    }
    let key = registry.set_default_instance(name)?;
    out::log(&format!("Set default instance to \"{key}\"."));
    Ok(())
}

fn cmd_rename(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        out::elog("Usage: quilltap instances rename <old> <new>");
        out::exit(1);
    }
    let (old_key, new_key) =
        InstanceRegistry::at_default_location().rename_instance(&args[0], &args[1])?;
    out::log(&format!("Renamed instance \"{old_key}\" → \"{new_key}\"."));
    Ok(())
}
