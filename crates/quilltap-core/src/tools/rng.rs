//! Port of v4's RNG tool (`lib/tools/rng-tool.ts` + `lib/tools/handlers/
//! rng-handler.ts`) — cryptographically-secure dice rolls, coin flips, and
//! spin-the-bottle (random participant selection).
//!
//! ## The randomness seam
//!
//! The roller primitives and their injected byte source live in
//! [`crate::pascal::dice`] — v4 moved them out of `rng-handler.ts` into
//! `lib/pascal/dice.ts` when Pascal's custom tools became a second roller, and
//! this port mirrors that move rather than rewriting them (which is what keeps
//! the byte-stream differential passing untouched). [`RandomBytes`],
//! [`OsRandomBytes`], and [`FixedBytes`] are re-exported here so the
//! `tools::rng` spelling keeps resolving; see `pascal::dice` for why the seam
//! exists at all.
//!
//! ## `type` — a number-or-string union
//!
//! v4's `RngType = number | 'flip_coin' | 'spin_the_bottle'`: a dice roll's type
//! IS the side count (a bare number in JSON). [`RngType`] serializes back to v4's
//! exact JSON shape (a number for dice, the two strings otherwise).

use serde::{Serialize, Serializer};
use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::{characters_read, chats_read, DbError};
use crate::pascal::dice::{flip_coin, roll_dice, secure_random_int};

pub use crate::pascal::dice::{FixedBytes, OsRandomBytes, RandomBytes};

/// v4 `RngType = number | 'flip_coin' | 'spin_the_bottle'`. A dice roll's type is
/// the number of sides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RngType {
    /// Dice with this many sides (v4: a bare number).
    Dice(u32),
    /// `'flip_coin'`.
    FlipCoin,
    /// `'spin_the_bottle'`.
    SpinTheBottle,
}

impl Serialize for RngType {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            RngType::Dice(n) => s.serialize_u32(*n),
            RngType::FlipCoin => s.serialize_str("flip_coin"),
            RngType::SpinTheBottle => s.serialize_str("spin_the_bottle"),
        }
    }
}

/// A single RNG result: a number (dice), `'heads'`/`'tails'` (coin), or a
/// participant name (spin the bottle). v4 `RngResult`.
#[derive(Debug, Clone, PartialEq)]
pub enum RngResultValue {
    Number(i64),
    Text(String),
}

impl Serialize for RngResultValue {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            RngResultValue::Number(n) => s.serialize_i64(*n),
            RngResultValue::Text(t) => s.serialize_str(t),
        }
    }
}

/// v4 `RngToolOutput`. `type` is a `serde_json::Value` because on a validation
/// failure v4 echoes the caller's raw `type` (which may be any value) or `6`;
/// on a success path it holds the serialized [`RngType`]. `sum`/`error` are
/// omitted when absent (v4's `undefined` dropped by `JSON.stringify`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RngToolOutput {
    pub success: bool,
    #[serde(rename = "type")]
    pub type_: Value,
    pub roll_count: u32,
    pub results: Vec<RngResultValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Context required for RNG tool execution (v4 `RngToolContext`).
#[derive(Debug, Clone)]
pub struct RngToolContext {
    pub user_id: String,
    pub chat_id: String,
}

// ---------------------------------------------------------------------------
// Participant resolution (spin the bottle).
// ---------------------------------------------------------------------------

/// v4 `getChatParticipants` — the active participants' character names, in
/// participant order. Reads the chat + each character through the ported repos
/// (matching v4's `repos.chats.findById` + `repos.characters.findById`). Returns
/// `[]` when the chat is missing or not owned by `user_id`, or when no active
/// participant resolves to a character.
fn get_chat_participants(db: &Db, chat_id: &str, user_id: &str) -> Result<Vec<String>, DbError> {
    let chat = db.read_main(|c| chats_read::find_by_id(c, chat_id))?;
    let chat = match chat {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };
    if chat.get("userId").and_then(Value::as_str) != Some(user_id) {
        return Ok(Vec::new());
    }
    let mut participants = Vec::new();
    if let Some(ps) = chat.get("participants").and_then(Value::as_array) {
        for p in ps {
            // Only active participants; all are type 'CHARACTER'.
            if p.get("isActive").and_then(Value::as_bool) != Some(true) {
                continue;
            }
            if let Some(cid) = p.get("characterId").and_then(Value::as_str) {
                let character = db.read_main(|main| {
                    db.read_mount_index(|mount| characters_read::find_by_id(main, mount, cid))
                })?;
                if let Some(ch) = character {
                    if let Some(name) = ch.get("name").and_then(Value::as_str) {
                        participants.push(name.to_string());
                    }
                }
            }
        }
    }
    Ok(participants)
}

/// v4 `spinTheBottle` — pick a random active participant, `count` times. One
/// participant fills every spin; zero participants is the `error` path.
fn spin_the_bottle(
    db: &Db,
    count: u32,
    chat_id: &str,
    user_id: &str,
    rng: &mut dyn RandomBytes,
) -> Result<(Vec<RngResultValue>, Option<String>), DbError> {
    let participants = get_chat_participants(db, chat_id, user_id)?;
    if participants.is_empty() {
        return Ok((
            Vec::new(),
            Some("No active participants found in chat".to_string()),
        ));
    }
    if participants.len() == 1 {
        let only = participants[0].clone();
        return Ok((
            (0..count)
                .map(|_| RngResultValue::Text(only.clone()))
                .collect(),
            None,
        ));
    }
    let mut results = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let index = secure_random_int(participants.len() as u64, rng) as usize - 1;
        results.push(RngResultValue::Text(participants[index].clone()));
    }
    Ok((results, None))
}

// ---------------------------------------------------------------------------
// Input validation.
// ---------------------------------------------------------------------------

/// The validated input: v4's `RngToolInput` (`{ type, rolls }`, `rolls` defaulting
/// to 1).
struct ValidatedRngInput {
    type_: RngType,
    rolls: u32,
}

/// v4 `validateRngInput` (the Zod `rngToolInputSchema`): `type` is an integer in
/// `[2, 1000]` OR `'flip_coin'`/`'spin_the_bottle'`; `rolls` is an optional
/// integer in `[1, 100]` (default 1). Returns `None` on any violation.
fn validate_rng_input(input: &Value) -> Option<ValidatedRngInput> {
    let obj = input.as_object()?;

    // type — number.int().min(2).max(1000) | enum('flip_coin','spin_the_bottle')
    let type_ = match obj.get("type") {
        Some(Value::Number(n)) => {
            let x = n.as_f64()?;
            // Zod .int() = Number.isInteger; range [2, 1000].
            if x.fract() != 0.0 || !(2.0..=1000.0).contains(&x) {
                return None;
            }
            RngType::Dice(x as u32)
        }
        Some(Value::String(s)) => match s.as_str() {
            "flip_coin" => RngType::FlipCoin,
            "spin_the_bottle" => RngType::SpinTheBottle,
            _ => return None,
        },
        _ => return None,
    };

    // rolls — number.int().min(1).max(100).default(1).optional(). Absent → 1; a
    // present non-number (incl. null — `.optional()` allows undefined, not null)
    // or out-of-range value fails validation.
    let rolls = match obj.get("rolls") {
        None => 1u32,
        Some(Value::Number(n)) => {
            let x = n.as_f64()?;
            if x.fract() != 0.0 || !(1.0..=100.0).contains(&x) {
                return None;
            }
            x as u32
        }
        Some(_) => return None,
    };

    Some(ValidatedRngInput { type_, rolls })
}

/// The `type` echoed in a failure output: v4's
/// `('type' in input) ? input.type : 6`.
fn failure_type(input: &Value) -> Value {
    match input.as_object().and_then(|o| o.get("type")) {
        Some(t) => t.clone(),
        None => json!(6),
    }
}

// ---------------------------------------------------------------------------
// The executor.
// ---------------------------------------------------------------------------

/// v4 `executeRngTool`. Validates the input, then dispatches on `type`: a number
/// rolls dice, `'flip_coin'` flips, `'spin_the_bottle'` selects a participant.
/// Never panics on bad input — an invalid input returns the exact v4 failure
/// output. Infra (DB) errors propagate as `Err` (v4 catches them into an error
/// output; the differential corpus never triggers one).
pub fn execute_rng_tool(
    db: &Db,
    input: &Value,
    context: &RngToolContext,
    rng: &mut dyn RandomBytes,
) -> Result<RngToolOutput, DbError> {
    let Some(valid) = validate_rng_input(input) else {
        return Ok(RngToolOutput {
            success: false,
            type_: failure_type(input),
            roll_count: 0,
            results: Vec::new(),
            sum: None,
            error: Some(
                "Invalid input: type must be a number (2-1000 for dice sides) or \"flip_coin\" or \"spin_the_bottle\""
                    .to_string(),
            ),
        });
    };

    let rolls = valid.rolls;
    match valid.type_ {
        RngType::Dice(sides) => {
            let (results, sum) = roll_dice(sides as u64, rolls, rng);
            Ok(RngToolOutput {
                success: true,
                type_: json!(sides),
                roll_count: rolls,
                results: results.into_iter().map(RngResultValue::Number).collect(),
                sum: Some(sum),
                error: None,
            })
        }
        RngType::FlipCoin => {
            // v4's `flipCoin` returns bare 'heads'/'tails' strings; the handler
            // lifts them into `RngResult[]` here.
            let results = flip_coin(rolls, rng)
                .into_iter()
                .map(RngResultValue::Text)
                .collect();
            Ok(RngToolOutput {
                success: true,
                type_: json!("flip_coin"),
                roll_count: rolls,
                results,
                sum: None,
                error: None,
            })
        }
        RngType::SpinTheBottle => {
            let (results, error) =
                spin_the_bottle(db, rolls, &context.chat_id, &context.user_id, rng)?;
            if let Some(err) = error {
                return Ok(RngToolOutput {
                    success: false,
                    type_: json!("spin_the_bottle"),
                    roll_count: 0,
                    results: Vec::new(),
                    sum: None,
                    error: Some(err),
                });
            }
            Ok(RngToolOutput {
                success: true,
                type_: json!("spin_the_bottle"),
                roll_count: rolls,
                results,
                sum: None,
                error: None,
            })
        }
    }
}

/// v4 `formatRngResults` — the byte-exact display/context string for an output.
pub fn format_rng_results(output: &RngToolOutput) -> String {
    if !output.success {
        return format!(
            "RNG Error: {}",
            output.error.as_deref().unwrap_or("Unknown error")
        );
    }

    let roll_count = output.roll_count;

    // Dice (type is a number).
    if let Some(sides) = output.type_.as_u64() {
        if roll_count == 1 {
            return format!("Rolled a d{sides}: **{}**", result_str(&output.results[0]));
        }
        let joined = join_results(&output.results);
        let sum = output.sum.unwrap_or(0);
        return format!("Rolled {roll_count}d{sides}: [{joined}] = **{sum}** total");
    }

    match output.type_.as_str() {
        Some("flip_coin") => {
            if roll_count == 1 {
                format!("Coin flip result: **{}**", result_str(&output.results[0]))
            } else {
                let heads = output
                    .results
                    .iter()
                    .filter(|r| matches!(r, RngResultValue::Text(t) if t == "heads"))
                    .count();
                let tails = output
                    .results
                    .iter()
                    .filter(|r| matches!(r, RngResultValue::Text(t) if t == "tails"))
                    .count();
                let joined = join_results(&output.results);
                format!("Flipped {roll_count} coins: [{joined}] ({heads} heads, {tails} tails)")
            }
        }
        Some("spin_the_bottle") => {
            if roll_count == 1 {
                format!(
                    "The bottle points to: **{}**",
                    result_str(&output.results[0])
                )
            } else {
                let joined = join_results(&output.results);
                format!("Spun the bottle {roll_count} times: [{joined}]")
            }
        }
        _ => format!("RNG result: {}", join_results(&output.results)),
    }
}

fn result_str(r: &RngResultValue) -> String {
    match r {
        RngResultValue::Number(n) => n.to_string(),
        RngResultValue::Text(t) => t.clone(),
    }
}

/// The JS `results.join(', ')` rendering (numbers stringified, strings as-is).
fn join_results(results: &[RngResultValue]) -> String {
    results
        .iter()
        .map(result_str)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // The `secure_random_int` byte-stream tests moved to `pascal::dice` with the
    // function itself (mirroring v4's own move into `lib/pascal/dice.ts`).

    #[test]
    fn format_single_and_multi_dice() {
        let single = RngToolOutput {
            success: true,
            type_: json!(20),
            roll_count: 1,
            results: vec![RngResultValue::Number(10)],
            sum: Some(10),
            error: None,
        };
        assert_eq!(format_rng_results(&single), "Rolled a d20: **10**");
        let multi = RngToolOutput {
            success: true,
            type_: json!(6),
            roll_count: 2,
            results: vec![RngResultValue::Number(4), RngResultValue::Number(5)],
            sum: Some(9),
            error: None,
        };
        assert_eq!(
            format_rng_results(&multi),
            "Rolled 2d6: [4, 5] = **9** total"
        );
    }

    #[test]
    fn format_coin_and_bottle_and_error() {
        let coin = RngToolOutput {
            success: true,
            type_: json!("flip_coin"),
            roll_count: 1,
            results: vec![RngResultValue::Text("heads".into())],
            sum: None,
            error: None,
        };
        assert_eq!(format_rng_results(&coin), "Coin flip result: **heads**");
        let bottle = RngToolOutput {
            success: true,
            type_: json!("spin_the_bottle"),
            roll_count: 1,
            results: vec![RngResultValue::Text("Friday".into())],
            sum: None,
            error: None,
        };
        assert_eq!(
            format_rng_results(&bottle),
            "The bottle points to: **Friday**"
        );
        let err = RngToolOutput {
            success: false,
            type_: json!("spin_the_bottle"),
            roll_count: 0,
            results: vec![],
            sum: None,
            error: Some("No active participants found in chat".into()),
        };
        assert_eq!(
            format_rng_results(&err),
            "RNG Error: No active participants found in chat"
        );
    }

    #[test]
    fn validation_rejects_out_of_range_and_bad_types() {
        assert!(validate_rng_input(&json!({"type": 1})).is_none()); // sides < 2
        assert!(validate_rng_input(&json!({"type": 1001})).is_none()); // sides > 1000
        assert!(validate_rng_input(&json!({"type": 6.5})).is_none()); // non-integer
        assert!(validate_rng_input(&json!({"type": "d6"})).is_none()); // bad string
        assert!(validate_rng_input(&json!({"type": 6, "rolls": 0})).is_none()); // rolls < 1
        assert!(validate_rng_input(&json!({"type": 6, "rolls": 101})).is_none()); // rolls > 100
        assert!(validate_rng_input(&json!({"type": 6})).is_some());
        assert!(validate_rng_input(&json!({"type": "flip_coin"})).is_some());
    }
}
