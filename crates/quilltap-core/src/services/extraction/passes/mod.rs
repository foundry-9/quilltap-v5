//! The four extraction passes — v4 `lib/memory/extraction/passes/*`
//! (`ac6348c5f`, bug 245's OTHER observer rules from `64743ee13`, bug 238's
//! fold-OTHER anchor from `93c5af6ad`; pin `83334b9a5`).
//!
//! v5 shape: one enum ([`ExtractionPass`]) instead of v4's object literals;
//! its arms are the four pass factories (`selfTurnPass(candidatePass)`,
//! `otherTurnPass()`, `otherFoldPass()`, `episodeFoldPass()`), and the episode
//! arm owns the run's `writtenEpisodeIds`, as v4's closure does, so ONE
//! instance per fold run spans every slice.

use std::collections::HashSet;

use crate::memory_candidate::CandidatePass;
use crate::memory_extraction::window::TranscriptWindow;
use crate::services::extraction::context::ExtractionContext;
use crate::services::extraction::run::{AdmittedCandidate, ExtractionPassName, PassCall};

pub mod episode_fold;
pub mod other_fold;
pub mod other_turn;
pub mod self_turn;
pub mod subjects;

/// v4 `ExtractionPass`.
#[derive(Clone, Debug)]
pub enum ExtractionPass {
    /// v4 `selfTurnPass(candidatePass = 'self-turn')` — `Carina` for an
    /// answer run on the SELF pass.
    SelfTurn {
        candidate_pass: CandidatePass,
    },
    OtherTurn,
    OtherFold,
    /// v4 `episodeFoldPass()` — remembers the episodes it has written, so an
    /// episode is never linked as a fragment of a later one in the same run.
    EpisodeFold {
        written_episode_ids: HashSet<String>,
    },
}

impl ExtractionPass {
    /// v4 `selfTurnPass()`.
    pub fn self_turn() -> Self {
        Self::SelfTurn {
            candidate_pass: CandidatePass::SelfTurn,
        }
    }

    /// v4 `episodeFoldPass()` — a fresh instance.
    pub fn episode_fold() -> Self {
        Self::EpisodeFold {
            written_episode_ids: HashSet::new(),
        }
    }

    pub fn name(&self) -> ExtractionPassName {
        match self {
            Self::SelfTurn { .. } => ExtractionPassName::SelfTurn,
            Self::OtherTurn => ExtractionPassName::OtherTurn,
            Self::OtherFold => ExtractionPassName::OtherFold,
            Self::EpisodeFold { .. } => ExtractionPassName::EpisodeFold,
        }
    }

    /// The candidate provenance (`carina` for an answer run on `self-turn`).
    pub fn candidate_pass(&self) -> CandidatePass {
        match self {
            Self::SelfTurn { candidate_pass } => *candidate_pass,
            other => crate::services::extraction::run::candidate_pass_of(other.name()),
        }
    }

    /// The cheap-LLM task type logged with every call.
    pub fn task_type(&self) -> &'static str {
        match self {
            Self::SelfTurn { .. } => "memory-extraction-self",
            Self::OtherTurn | Self::OtherFold => "memory-extraction-other",
            Self::EpisodeFold { .. } => "fold-episode-extraction",
        }
    }

    /// The calls over a window (none when the window holds nothing for this
    /// pass).
    pub fn calls(&self, ctx: &ExtractionContext, window: &TranscriptWindow) -> Vec<PassCall> {
        match self {
            Self::SelfTurn { .. } => self_turn::calls(ctx, window),
            Self::OtherTurn => other_turn::calls(ctx, window),
            Self::OtherFold => other_fold::calls(ctx, window),
            Self::EpisodeFold { .. } => episode_fold::calls(ctx, window),
        }
    }

    /// After a batch's candidates are admitted (the episode pass links its
    /// fragments here).
    pub async fn after_batch(
        &mut self,
        ctx: &ExtractionContext,
        window: &TranscriptWindow,
        admitted: &[AdmittedCandidate],
    ) {
        if let Self::EpisodeFold {
            written_episode_ids,
        } = self
        {
            episode_fold::after_batch(written_episode_ids, ctx, window, admitted).await;
        }
    }
}
