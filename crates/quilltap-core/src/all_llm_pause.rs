//! Port of v4's lib/chat/turn-manager/all-llm-pause.ts — the logarithmic
//! auto-pause thresholds for all-LLM chats (3, 6, 12, 24, 48, … turns), which
//! cap runaway API usage when no human is in the loop.

/// Turns before the first pause; the sequence doubles from here.
pub const INITIAL_PAUSE_INTERVAL: i64 = 3;

/// The next interval via logarithmic doubling: `0 → 3`, otherwise `current * 2`.
pub fn get_next_pause_interval(current_interval: i64) -> i64 {
    if current_interval == 0 {
        INITIAL_PAUSE_INTERVAL
    } else {
        current_interval * 2
    }
}

/// Whether the chat should pause at this turn count — true exactly when the
/// count equals a threshold (3, 6, 12, 24, …). Non-positive counts never pause.
pub fn should_pause_for_all_llm(turn_count: i64) -> bool {
    if turn_count <= 0 {
        return false;
    }
    let mut threshold = INITIAL_PAUSE_INTERVAL;
    while threshold <= turn_count {
        if turn_count == threshold {
            return true;
        }
        threshold *= 2;
    }
    false
}

/// The most recent threshold at or below `turn_count` (the last pause that was
/// or should have been reached), or 0 when below the first threshold.
pub fn get_current_pause_threshold(turn_count: i64) -> i64 {
    if turn_count < INITIAL_PAUSE_INTERVAL {
        return 0;
    }
    let mut threshold = INITIAL_PAUSE_INTERVAL;
    let mut last_threshold = 0;
    while threshold <= turn_count {
        last_threshold = threshold;
        threshold *= 2;
    }
    last_threshold
}

/// The next threshold strictly above `turn_count`.
pub fn get_next_pause_threshold(turn_count: i64) -> i64 {
    let mut threshold = INITIAL_PAUSE_INTERVAL;
    while threshold <= turn_count {
        threshold *= 2;
    }
    threshold
}

/// How many turns remain until the next pause.
pub fn get_turns_until_next_pause(turn_count: i64) -> i64 {
    get_next_pause_threshold(turn_count) - turn_count
}
