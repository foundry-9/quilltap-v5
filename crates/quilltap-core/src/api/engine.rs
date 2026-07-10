//! `CoreEngine` — the engine-backed [`QuilltapCore`](super::QuilltapCore)
//! implementation: the one place `Request`s become engine calls.
//!
//! The engine has two macro-states mirroring v4's locked mode:
//!
//! - **Locked** — the pepper is not operational (`needs-setup` /
//!   `needs-passphrase`). Only the always-available family answers
//!   (health, unlock state, unlock, instances); everything else gets the
//!   readiness refusal (D2).
//! - **Ready** — the pepper resolved; the instance's databases are open (one
//!   [`Db`] per the single-writer runtime) and the host's
//!   [`EngineAssembler`] has wired its drivers (job pump, cadence loops).
//!
//! The **assembler seam** is how the composition root stays in
//! `quilltap-host` while the state machine lives here: whenever the pepper
//! becomes operational (at boot, or later via `Unlock`), the engine opens the
//! `Db` and hands it to the assembler, which builds the runner/registry,
//! spawns its cadence tasks, and returns a shutdown handle. `Lock` calls that
//! handle, drops the `Db`, and returns to `needs-passphrase` — faithful to v4
//! `lockDbKey` (including the consequence that an env-pepper-booted,
//! vault-less instance cannot re-unlock; v4 behaves identically).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::sync::broadcast;

use crate::db::chat_settings;
use crate::db::runtime::{Db, DbPaths};
use crate::dbkey::{self, DbKeyError};
use crate::services::creation_progress::CreationProgressBus;
use crate::services::provisioning;

use super::chat_create::{ChatCreateDriver, ChatCreateDriverRequest};
use super::chat_send::{ChatSendDriver, ChatSendRequest};
use super::provision::{provision, PepperHashMismatch};
use super::types::{
    AckDto, ErrorKind, Event, HealthDto, PepperState, Request, Response, SetupResultDto,
    UnlockStateDto,
};
use super::{InstanceDirectory, QuilltapCore};

/// v4 `SINGLE_USER_ID` (`lib/auth/single-user.ts`) — the synthetic single
/// user every row belongs to. D2: the session layer does not port.
pub const SINGLE_USER_ID: &str = "ffffffff-ffff-ffff-ffff-ffffffffffff";

/// v4 `handleSetup`'s one-shot save-the-pepper message (shown once, verbatim).
const SETUP_MESSAGE: &str =
    "Encryption key generated and stored. Save this value — it will not be displayed again.";

/// Broadcast capacity for the event channel. A slow subscriber past this
/// lags (drops oldest) rather than blocking the engine — the SSE transport
/// treats a lag as a resync signal.
const EVENT_CHANNEL_CAPACITY: usize = 1024;

// ============================================================================
// Config + the host seams
// ============================================================================

/// Engine construction inputs. `base_dir` is the instance root (v4
/// `getBaseDataDir()`); the databases and `.dbkey` live in `<base>/data`
/// (v4 `lib/paths.ts` layout).
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub base_dir: PathBuf,
    /// Reported by `Health` (the host supplies its crate version).
    pub version: String,
    /// The `ENCRYPTION_MASTER_PEPPER` env pepper, if set (host reads env).
    pub env_pepper: Option<String>,
}

impl CoreConfig {
    /// `<base>/data` — where the databases and `quilltap.dbkey` live.
    pub fn data_dir(&self) -> PathBuf {
        self.base_dir.join("data")
    }
}

/// One assembly's products (P4.2 — grown from the bare shutdown handle): the
/// teardown handle plus the optional chat-send driver. `chat_send: None` keeps
/// read-only embedders (and the P4.0 tests) valid — the `ChatSend` dispatch
/// arm answers "chat dispatch not assembled".
pub struct EngineAssembly {
    pub shutdown: Box<dyn EngineShutdown>,
    pub chat_send: Option<Arc<dyn ChatSendDriver>>,
    /// The chat-creation driver (P4.4u2b); `None` keeps read-only embedders (and
    /// the P4.0 tests) valid — the `ChatCreate` arm answers "not assembled".
    pub chat_create: Option<Arc<dyn ChatCreateDriver>>,
}

impl EngineAssembly {
    /// A driver-less assembly around a shutdown handle.
    pub fn shutdown_only(shutdown: Box<dyn EngineShutdown>) -> Self {
        EngineAssembly {
            shutdown,
            chat_send: None,
            chat_create: None,
        }
    }
}

/// The composition-root seam: called whenever the pepper becomes operational
/// (boot or unlock) with the freshly opened [`Db`] and the engine's event
/// broadcast (the spine's frames ride it). The host clones the `Db` into its
/// drivers (job runner, cadence loops), spawns them, and returns the assembly
/// that tears them down again on `Lock`. Must be reusable — a lock/unlock
/// cycle assembles more than once.
///
/// `pepper` + `data_dir` are threaded through because the chat-create driver
/// opens its OWN writable [`Writer`](crate::db::Writer)s per create (the outfit
/// sub-unit holds writable connections across an LLM await, which the sync
/// single-writer closure cannot host); `bus` is the shared
/// [`CreationProgressBus`] the driver's emitter publishes to and the transport
/// replays from.
pub trait EngineAssembler: Send + Sync {
    fn assemble(
        &self,
        db: &Db,
        events: &broadcast::Sender<Event>,
        pepper: &str,
        data_dir: &std::path::Path,
        bus: &Arc<CreationProgressBus>,
    ) -> Result<EngineAssembly, String>;
}

/// Tears down one assembly (stop loops, drop `Db` clones). Idempotent.
pub trait EngineShutdown: Send + Sync {
    fn shutdown(&self);
}

/// A no-driver assembler for tests and read-only embedders.
pub struct NoopAssembler;

struct NoopShutdown;
impl EngineShutdown for NoopShutdown {
    fn shutdown(&self) {}
}

impl EngineAssembler for NoopAssembler {
    fn assemble(
        &self,
        _db: &Db,
        _events: &broadcast::Sender<Event>,
        _pepper: &str,
        _data_dir: &std::path::Path,
        _bus: &Arc<CreationProgressBus>,
    ) -> Result<EngineAssembly, String> {
        Ok(EngineAssembly::shutdown_only(Box::new(NoopShutdown)))
    }
}

// ============================================================================
// The engine
// ============================================================================

/// Boot failures — hard errors before the engine exists (a locked vault is
/// NOT a failure; the engine boots into the locked state and serves the
/// unlock family).
#[derive(Debug)]
pub enum BootError {
    /// The env pepper contradicts the `.dbkey` hash (v4's FATAL exit).
    PepperMismatch(PepperHashMismatch),
    /// The pepper resolved but `<data>/quilltap.db` does not exist. Opening
    /// would CREATE an empty cipher-keyed file — schema creation (migrations)
    /// is unported P4.4 surface, so a missing main DB is refused instead.
    MissingMainDb(PathBuf),
    Db(crate::db::DbError),
    Assemble(String),
}

impl std::fmt::Display for BootError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BootError::PepperMismatch(e) => write!(f, "{e}"),
            BootError::MissingMainDb(p) => {
                write!(
                    f,
                    "no main database at {} (fresh-instance creation is not yet supported)",
                    p.display()
                )
            }
            BootError::Db(e) => write!(f, "database open failed: {e}"),
            BootError::Assemble(e) => write!(f, "engine assembly failed: {e}"),
        }
    }
}
impl std::error::Error for BootError {}

enum EngineState {
    Locked {
        pepper_state: PepperState,
        has_user_passphrase: bool,
    },
    Ready(ReadyEngine),
}

struct ReadyEngine {
    db: Db,
    pepper_state: PepperState,
    has_user_passphrase: bool,
    shutdown: Box<dyn EngineShutdown>,
    /// The assembly's chat-send driver (P4.2); `None` for read-only embedders.
    chat_send: Option<Arc<dyn ChatSendDriver>>,
    /// The assembly's chat-create driver (P4.4u2b); `None` for read-only embedders.
    chat_create: Option<Arc<dyn ChatCreateDriver>>,
}

/// The engine-backed `QuilltapCore`. Cloneable (`Arc` inside) so every
/// transport and driver can hold one handle.
#[derive(Clone)]
pub struct CoreEngine {
    inner: Arc<EngineInner>,
}

struct EngineInner {
    config: CoreConfig,
    assembler: Box<dyn EngineAssembler>,
    instances: Arc<dyn InstanceDirectory>,
    /// std Mutex — never held across an await (all state work is sync).
    state: Mutex<EngineState>,
    events: broadcast::Sender<Event>,
    /// The Green-Room replay buffer (D6) — one per engine, shared by the
    /// chat-create driver's emitter (publish) and the transport (replay).
    creation_progress_bus: Arc<CreationProgressBus>,
}

impl CoreEngine {
    /// Provision the pepper and boot. An operational pepper opens the
    /// databases and assembles the drivers; a locked vault boots into the
    /// locked state (serving the unlock family). Hard failures — pepper/hash
    /// mismatch, missing main DB, open/assembly errors — refuse the boot.
    pub fn boot(
        config: CoreConfig,
        assembler: Box<dyn EngineAssembler>,
        instances: Arc<dyn InstanceDirectory>,
    ) -> Result<CoreEngine, BootError> {
        let provisioned = provision(&config.data_dir(), config.env_pepper.as_deref())
            .map_err(BootError::PepperMismatch)?;

        let (events, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        let inner = EngineInner {
            config,
            assembler,
            instances,
            state: Mutex::new(EngineState::Locked {
                pepper_state: provisioned.state,
                has_user_passphrase: provisioned.has_user_passphrase,
            }),
            events,
            creation_progress_bus: Arc::new(CreationProgressBus::new()),
        };

        if provisioned.state.is_operational() {
            let ready = open_ready(
                &inner,
                provisioned
                    .pepper
                    .as_deref()
                    .expect("operational state carries the pepper"),
                provisioned.state,
                provisioned.has_user_passphrase,
            )?;
            *inner.state.lock().unwrap() = EngineState::Ready(ready);
        }

        Ok(CoreEngine {
            inner: Arc::new(inner),
        })
    }

    /// The event publisher — later units (chat send, creation progress) emit
    /// through this; transports subscribe via [`QuilltapCore::subscribe`].
    pub fn event_sender(&self) -> &broadcast::Sender<Event> {
        &self.inner.events
    }

    /// The Green-Room replay buffer (D6). The transport drains
    /// [`CreationProgressBus::active_snapshot`] onto each new `/api/events`
    /// stream so a late-connecting creation dialog replays the buffered
    /// backlog; the chat-create driver's emitter publishes into the same bus.
    pub fn creation_progress_bus(&self) -> Arc<CreationProgressBus> {
        Arc::clone(&self.inner.creation_progress_bus)
    }

    /// The open `Db`, when ready (a clone — the runtime handle is `Clone`).
    pub fn db(&self) -> Option<Db> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Some(r.db.clone()),
            EngineState::Locked { .. } => None,
        }
    }

    async fn dispatch_impl(&self, req: Request) -> Response {
        match req {
            Request::Health => self.health(),
            Request::UnlockState => self.unlock_state(),
            Request::Unlock { passphrase } => self.unlock(&passphrase),
            Request::Setup { passphrase } => self.setup(&passphrase),
            Request::StorePepper { passphrase } => self.store_pepper(&passphrase),
            Request::ChangePassphrase {
                old_passphrase,
                new_passphrase,
            } => self.change_passphrase(&old_passphrase, &new_passphrase),
            Request::Lock => self.lock(),
            Request::ListInstances => match self.inner.instances.list() {
                Ok(dto) => Response::Instances(dto),
                Err(e) => Response::error(ErrorKind::Internal, e),
            },
            Request::ListChats {
                exclude_tag_ids,
                limit,
                include_autonomous,
            } => match self.ready_db() {
                Ok(db) => super::salon::list_chats(
                    &db,
                    SINGLE_USER_ID,
                    &exclude_tag_ids,
                    limit,
                    include_autonomous,
                ),
                Err(r) => r,
            },
            Request::ChatGet { chat_id } => match self.ready_db() {
                Ok(db) => super::salon::chat_get(&db, SINGLE_USER_ID, &chat_id).await,
                Err(r) => r,
            },
            Request::ChatSettings => match self.ready_db() {
                Ok(db) => super::settings::chat_settings_get(&db, SINGLE_USER_ID).await,
                Err(r) => r,
            },
            Request::ChatSettingsUpdate { settings } => match self.ready_db() {
                Ok(db) => {
                    super::settings::chat_settings_update(&db, SINGLE_USER_ID, &settings).await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileList { image_capable } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_list(&db, SINGLE_USER_ID, image_capable)
                }
                Err(r) => r,
            },
            Request::ConnectionProfileCreate { profile } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_create(&db, SINGLE_USER_ID, &profile).await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileUpdate {
                profile_id,
                profile,
            } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_update(
                        &db,
                        SINGLE_USER_ID,
                        &profile_id,
                        &profile,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileDelete { profile_id } => match self.ready_db() {
                Ok(db) => super::settings::connection_profile_delete(&db, &profile_id).await,
                Err(r) => r,
            },
            Request::ConnectionProfileReorder { ordered_ids } => match self.ready_db() {
                Ok(db) => {
                    super::settings::connection_profile_reorder(&db, SINGLE_USER_ID, &ordered_ids)
                        .await
                }
                Err(r) => r,
            },
            Request::ConnectionProfileResetSort => match self.ready_db() {
                Ok(db) => super::settings::connection_profile_reset_sort(&db, SINGLE_USER_ID).await,
                Err(r) => r,
            },
            // The wire actions need a host provider-actions driver (validator /
            // completion / models-fetch seam) — a tracked deferral (the swipe-
            // generate precedent). The handler functions ARE ported and
            // differential-verified; the live wiring lands at P4.6e unification.
            Request::ConnectionProfileTest { .. }
            | Request::ConnectionProfileTestMessage { .. }
            | Request::ApiKeyTest { .. }
            | Request::ModelFetch { .. } => match self.ready_db() {
                Ok(_) => Response::error(
                    ErrorKind::Internal,
                    "provider wire actions not assembled (provider-actions driver deferral)",
                ),
                Err(r) => r,
            },
            Request::ApiKeyList => match self.ready_db() {
                Ok(db) => super::settings::api_key_list(&db, SINGLE_USER_ID),
                Err(r) => r,
            },
            Request::ApiKeyCreate {
                label,
                provider,
                api_key,
            } => match self.ready_db() {
                Ok(db) => {
                    super::settings::api_key_create(
                        &db,
                        SINGLE_USER_ID,
                        &provider,
                        &label,
                        &api_key,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ApiKeyUpdate {
                api_key_id,
                label,
                is_active,
                api_key,
            } => match self.ready_db() {
                Ok(db) => {
                    super::settings::api_key_update(
                        &db,
                        &api_key_id,
                        label.as_deref(),
                        is_active,
                        api_key.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ApiKeyDelete { api_key_id } => match self.ready_db() {
                Ok(db) => super::settings::api_key_delete(&db, &api_key_id).await,
                Err(r) => r,
            },
            Request::ProviderList => match self.ready_db() {
                Ok(_) => super::settings::provider_list(),
                Err(r) => r,
            },
            Request::ModelList { provider } => match self.ready_db() {
                Ok(db) => super::settings::model_list(&db, provider.as_deref()),
                Err(r) => r,
            },
            Request::ChatTurnAction {
                chat_id,
                action,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::turn_action(
                        &db,
                        &chat_id,
                        &action,
                        participant_id.as_deref(),
                        random_f64(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MessageEdit {
                message_id,
                content,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::message_edit(&db, SINGLE_USER_ID, &message_id, &content).await
                }
                Err(r) => r,
            },
            Request::MessageDelete {
                message_id,
                memory_action,
                skip_confirmation,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::message_delete(
                        &db,
                        SINGLE_USER_ID,
                        &message_id,
                        memory_action.as_deref(),
                        skip_confirmation,
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::MessageSwipe {
                message_id,
                swipe_index,
            } => match self.ready_db() {
                Ok(db) => match swipe_index {
                    Some(idx) => {
                        super::salon::message_swipe_switch(&db, SINGLE_USER_ID, &message_id, idx)
                    }
                    // The generate branch needs the model driver (tracked deferral).
                    None => Response::error(
                        ErrorKind::Internal,
                        "swipe generation not yet assembled (model driver deferral)",
                    ),
                },
                Err(r) => r,
            },
            Request::ChatUpdate { chat_id, chat } => match self.ready_db() {
                Ok(db) => super::salon::chat_update(&db, &chat_id, &chat).await,
                Err(r) => r,
            },
            Request::ChatImpersonate {
                chat_id,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => super::salon::chat_impersonate(&db, &chat_id, &participant_id).await,
                Err(r) => r,
            },
            Request::ChatStopImpersonate {
                chat_id,
                participant_id,
                new_connection_profile_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::chat_stop_impersonate(
                        &db,
                        &chat_id,
                        &participant_id,
                        new_connection_profile_id.as_deref(),
                    )
                    .await
                }
                Err(r) => r,
            },
            Request::ChatSetActiveSpeaker {
                chat_id,
                participant_id,
            } => match self.ready_db() {
                Ok(db) => {
                    super::salon::chat_set_active_speaker(&db, &chat_id, &participant_id).await
                }
                Err(r) => r,
            },
            Request::ChatSend {
                chat_id,
                content,
                continue_mode,
                responding_participant_id,
                target_participant_ids,
                speaking_as_participant_id,
                file_ids,
                nudge,
                pending_tool_results,
            } => {
                // v4 `sendMessageSchema` superRefine: a normal (non-continue) send
                // with blank content, no files, and no tool results is rejected.
                if !continue_mode
                    && content.trim().is_empty()
                    && file_ids.is_empty()
                    && pending_tool_results.is_empty()
                {
                    return Response::error(
                        ErrorKind::BadRequest,
                        "Message must have content, attached files, or tool results",
                    );
                }
                self.chat_send(ChatSendRequest {
                    chat_id,
                    content,
                    continue_mode,
                    responding_participant_id,
                    target_participant_ids,
                    speaking_as_participant_id,
                    file_ids,
                    nudge,
                    pending_tool_results,
                })
                .await
            }
            Request::ChatCreate { request } => {
                self.chat_create(ChatCreateDriverRequest {
                    raw: serde_json::Value::Object(request),
                })
                .await
            }
        }
    }

    /// Extract the open `Db` under the readiness gate (D2). `Err` is the locked
    /// refusal the caller returns directly.
    fn ready_db(&self) -> Result<Db, Response> {
        match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => Ok(r.db.clone()),
            EngineState::Locked { pepper_state, .. } => Err(Response::locked(*pepper_state)),
        }
    }

    /// The `ChatCreate` arm: readiness-gated (D2), then delegated to the
    /// assembly's driver (mirrors [`Self::chat_send`]). A ready engine without a
    /// driver (read-only embedder) answers a plain internal error.
    async fn chat_create(&self, req: ChatCreateDriverRequest) -> Response {
        let driver = {
            let state = self.inner.state.lock().unwrap();
            match &*state {
                EngineState::Ready(r) => match &r.chat_create {
                    Some(d) => Arc::clone(d),
                    None => {
                        return Response::error(ErrorKind::Internal, "chat create not assembled");
                    }
                },
                EngineState::Locked { pepper_state, .. } => {
                    return Response::locked(*pepper_state);
                }
            }
        };
        match driver.create(req).await {
            Ok(dto) => Response::ChatCreate(dto),
            Err(e) => Response::Error(e),
        }
    }

    /// The `ChatSend` arm: readiness-gated (D2), then delegated to the
    /// assembly's driver. A ready engine without a driver (read-only embedder)
    /// answers a plain internal error.
    async fn chat_send(&self, req: ChatSendRequest) -> Response {
        let driver = {
            let state = self.inner.state.lock().unwrap();
            match &*state {
                EngineState::Ready(r) => match &r.chat_send {
                    Some(d) => Arc::clone(d),
                    None => {
                        return Response::error(ErrorKind::Internal, "chat dispatch not assembled");
                    }
                },
                EngineState::Locked { pepper_state, .. } => {
                    return Response::locked(*pepper_state);
                }
            }
        };
        match driver.send(req).await {
            Ok(dto) => Response::ChatSend(dto),
            Err(e) => Response::Error(e),
        }
    }

    fn health(&self) -> Response {
        let (ready, pepper_state) = match &*self.inner.state.lock().unwrap() {
            EngineState::Ready(r) => (true, r.pepper_state),
            EngineState::Locked { pepper_state, .. } => (false, *pepper_state),
        };
        Response::Health(HealthDto {
            status: "ok".to_string(),
            version: self.inner.config.version.clone(),
            ready,
            pepper_state,
        })
    }

    /// v4 `GET /api/v1/system/unlock`: state + hasUserPassphrase, plus
    /// autoLockMinutes when unlocked and the user's auto-lock is enabled
    /// (that read is error-swallowed, as v4's is).
    fn unlock_state(&self) -> Response {
        let state = self.inner.state.lock().unwrap();
        let dto = match &*state {
            EngineState::Locked {
                pepper_state,
                has_user_passphrase,
            } => UnlockStateDto {
                state: *pepper_state,
                has_user_passphrase: *has_user_passphrase,
                auto_lock_minutes: None,
            },
            EngineState::Ready(r) => UnlockStateDto {
                state: r.pepper_state,
                has_user_passphrase: r.has_user_passphrase,
                auto_lock_minutes: read_auto_lock_minutes(&r.db),
            },
        };
        Response::UnlockState(dto)
    }

    /// v4 `?action=unlock`. Only meaningful from `needs-passphrase`; the
    /// 3-attempt limit and auto-lock resume are P4.4 (the full unlock
    /// service).
    fn unlock(&self, passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &*state {
            EngineState::Ready(_) => Response::error(ErrorKind::BadRequest, "Already unlocked"),
            EngineState::Locked {
                pepper_state: PepperState::NeedsSetup,
                ..
            } => Response::error(
                ErrorKind::BadRequest,
                "No database key is configured; setup is required first",
            ),
            EngineState::Locked {
                pepper_state,
                has_user_passphrase,
            } => {
                let pepper_state = *pepper_state;
                let has_user_passphrase = *has_user_passphrase;
                debug_assert_eq!(pepper_state, PepperState::NeedsPassphrase);
                match dbkey::load_pepper(&self.inner.config.data_dir(), Some(passphrase)) {
                    Ok(pepper) => match open_ready(
                        &self.inner,
                        &pepper,
                        PepperState::Resolved,
                        has_user_passphrase,
                    ) {
                        Ok(ready) => {
                            let dto = UnlockStateDto {
                                state: ready.pepper_state,
                                has_user_passphrase: ready.has_user_passphrase,
                                auto_lock_minutes: read_auto_lock_minutes(&ready.db),
                            };
                            *state = EngineState::Ready(ready);
                            Response::UnlockState(dto)
                        }
                        Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                    },
                    Err(DbKeyError::DecryptFailed) => {
                        Response::error(ErrorKind::BadRequest, "Invalid passphrase")
                    }
                    Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                }
            }
        }
    }

    /// v4 `?action=setup` (`handleSetup` + `setupDbKey` + fresh-instance
    /// provisioning): mint a pepper, write `quilltap.dbkey`, create a fresh
    /// **encrypted-from-byte-zero** instance (schema + baseline seed), assemble,
    /// and return the pepper ONCE. Only valid from `needs-setup`.
    ///
    /// v4's post-setup plaintext→cipher conversion is a **named non-port**: v4
    /// creates the DB plaintext during pre-setup migrations, so setup must encrypt
    /// it in place; v5 has no plaintext window (the pepper is in hand before any
    /// partition is created), so there is nothing to convert.
    fn setup(&self, passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &*state {
            EngineState::Ready(_) => Response::error(
                ErrorKind::BadRequest,
                "Already set up (the vault is unlocked)",
            ),
            EngineState::Locked {
                pepper_state: PepperState::NeedsSetup,
                ..
            } => {
                let data_dir = self.inner.config.data_dir();
                let pepper = match dbkey::generate_pepper() {
                    Ok(p) => p,
                    Err(e) => return Response::error(ErrorKind::Internal, e.to_string()),
                };
                // Write quilltap.dbkey (main only — v4 setup writes just the main
                // .dbkey; the llm-logs .dbkey lands on the first passphrase change).
                if let Err(e) = dbkey::save_dbkey(&data_dir, &pepper, passphrase) {
                    return Response::error(ErrorKind::Internal, e.to_string());
                }
                // Provision the schema + baseline seed, encrypted from creation.
                if let Err(e) = provisioning::provision_fresh_instance(&data_dir, &pepper) {
                    return Response::error(ErrorKind::Internal, e.to_string());
                }
                let has_user_passphrase = !passphrase.is_empty();
                match open_ready(
                    &self.inner,
                    &pepper,
                    PepperState::Resolved,
                    has_user_passphrase,
                ) {
                    Ok(ready) => {
                        *state = EngineState::Ready(ready);
                        Response::Setup(SetupResultDto {
                            pepper,
                            message: SETUP_MESSAGE.to_string(),
                        })
                    }
                    Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                }
            }
            EngineState::Locked { .. } => Response::error(
                ErrorKind::BadRequest,
                "Setup is only available on a fresh instance",
            ),
        }
    }

    /// v4 `?action=store` (`storeEnvPepperInDbKey`): persist the env-provided
    /// pepper into `quilltap.dbkey`, the `needs-vault-storage` → `resolved`
    /// transition. The engine is already Ready (an env pepper is operational); this
    /// just writes the file and advances the reported state.
    fn store_pepper(&self, passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &mut *state {
            EngineState::Ready(r) if r.pepper_state == PepperState::NeedsVaultStorage => {
                let pepper = match self.inner.config.env_pepper.as_deref() {
                    Some(p) if !p.is_empty() => p,
                    _ => {
                        return Response::error(
                            ErrorKind::Internal,
                            "No pepper in environment to store",
                        )
                    }
                };
                if let Err(e) = dbkey::save_dbkey(&self.inner.config.data_dir(), pepper, passphrase)
                {
                    return Response::error(ErrorKind::Internal, e.to_string());
                }
                r.pepper_state = PepperState::Resolved;
                r.has_user_passphrase = !passphrase.is_empty();
                Response::Ack(AckDto::default())
            }
            EngineState::Ready(_) => Response::error(
                ErrorKind::BadRequest,
                "The pepper is already stored in a .dbkey file",
            ),
            EngineState::Locked { pepper_state, .. } => Response::locked(*pepper_state),
        }
    }

    /// v4 `?action=change-passphrase` (`changePassphrase`): re-wrap the pepper
    /// under a new passphrase (no DB re-encryption). Only valid when `resolved`
    /// (v4 requires exactly that state — `needs-vault-storage` has no .dbkey to
    /// re-wrap). Either passphrase may be empty (the no-passphrase sentinel).
    fn change_passphrase(&self, old_passphrase: &str, new_passphrase: &str) -> Response {
        let mut state = self.inner.state.lock().unwrap();
        match &mut *state {
            EngineState::Ready(r) if r.pepper_state == PepperState::Resolved => {
                match dbkey::change_passphrase(
                    &self.inner.config.data_dir(),
                    old_passphrase,
                    new_passphrase,
                ) {
                    Ok(_) => {
                        r.has_user_passphrase = !new_passphrase.is_empty();
                        Response::Ack(AckDto::default())
                    }
                    Err(DbKeyError::DecryptFailed) => {
                        Response::error(ErrorKind::Unauthorized, "Current passphrase is incorrect")
                    }
                    Err(DbKeyError::NotFound(_)) => {
                        Response::error(ErrorKind::BadRequest, "No .dbkey file found")
                    }
                    Err(e) => Response::error(ErrorKind::Internal, e.to_string()),
                }
            }
            EngineState::Ready(_) => Response::error(
                ErrorKind::BadRequest,
                "Application must be unlocked before changing the passphrase",
            ),
            EngineState::Locked { .. } => Response::error(
                ErrorKind::BadRequest,
                "Application must be unlocked before changing the passphrase",
            ),
        }
    }

    /// v4 `?action=lock` / auto-lock (`lockDbKey`): tear the drivers down,
    /// drop the `Db`, return to `needs-passphrase`. Idempotent when already
    /// locked (answers the current state).
    fn lock(&self) -> Response {
        // Scope the guard: `unlock_state` below re-acquires the (non-reentrant)
        // state mutex, so it must be released first.
        {
            let mut state = self.inner.state.lock().unwrap();
            if let EngineState::Ready(r) = &*state {
                let has_user_passphrase = r.has_user_passphrase;
                let old = std::mem::replace(
                    &mut *state,
                    EngineState::Locked {
                        pepper_state: PepperState::NeedsPassphrase,
                        has_user_passphrase,
                    },
                );
                if let EngineState::Ready(r) = old {
                    r.shutdown.shutdown();
                    // r.db drops here; once the drivers' clones are gone too,
                    // the writer thread exits.
                }
            }
        }
        self.unlock_state()
    }
}

/// A uniform random `f64` in `[0, 1)` (v4 `Math.random()`), sourced from the OS
/// CSPRNG. Used by the turn-action next-speaker selection (the engine's real
/// clock/RNG; the differential harness injects a pinned value).
fn random_f64() -> f64 {
    let mut bytes = [0u8; 8];
    getrandom::getrandom(&mut bytes).expect("getrandom");
    let n = u64::from_le_bytes(bytes);
    // 53-bit mantissa → uniform [0,1).
    (n >> 11) as f64 / (1u64 << 53) as f64
}

impl QuilltapCore for CoreEngine {
    fn dispatch(&self, req: Request) -> impl std::future::Future<Output = Response> + Send {
        self.dispatch_impl(req)
    }

    fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.inner.events.subscribe()
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Open the instance's databases and run the host assembler. Main is
/// required; the sibling partitions are opened when their files exist.
fn open_ready(
    inner: &EngineInner,
    pepper: &str,
    pepper_state: PepperState,
    has_user_passphrase: bool,
) -> Result<ReadyEngine, BootError> {
    let data = inner.config.data_dir();
    let main = data.join("quilltap.db");
    if !main.exists() {
        return Err(BootError::MissingMainDb(main));
    }
    let optional = |name: &str| -> Option<PathBuf> {
        let p = data.join(name);
        p.exists().then_some(p)
    };
    let paths = DbPaths {
        main,
        mount_index: optional("quilltap-mount-index.db"),
        llm_logs: optional("quilltap-llm-logs.db"),
    };
    let db = Db::open(paths, pepper).map_err(BootError::Db)?;
    let assembly = inner
        .assembler
        .assemble(
            &db,
            &inner.events,
            pepper,
            &data,
            &inner.creation_progress_bus,
        )
        .map_err(BootError::Assemble)?;
    Ok(ReadyEngine {
        db,
        pepper_state,
        has_user_passphrase,
        shutdown: assembly.shutdown,
        chat_send: assembly.chat_send,
        chat_create: assembly.chat_create,
    })
}

/// The auto-lock read behind the unlock-state DTO (v4: chatSettings
/// `autoLockSettings.enabled ? idleMinutes : null`, error-swallowed).
fn read_auto_lock_minutes(db: &Db) -> Option<f64> {
    let settings = db
        .read_main(|conn| chat_settings::find_by_user_id(conn, SINGLE_USER_ID))
        .ok()??;
    let auto = settings.get("autoLockSettings")?;
    if auto.get("enabled").and_then(|v| v.as_bool()) == Some(true) {
        auto.get("idleMinutes").and_then(|v| v.as_f64())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Writer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tempfile::{tempdir, TempDir};

    const PEPPER: &str = "dGVzdHBlcHBlcnRlc3RwZXBwZXJ0ZXN0cGVwcGVyMDE=";

    struct EmptyInstances;
    impl InstanceDirectory for EmptyInstances {
        fn list(&self) -> Result<super::super::types::InstancesDto, String> {
            Ok(super::super::types::InstancesDto {
                instances: vec![],
                default_instance: None,
            })
        }
    }

    /// Records assemble/shutdown counts so the lock/unlock cycle is provable.
    struct CountingAssembler {
        assembled: Arc<AtomicUsize>,
        shutdowns: Arc<AtomicUsize>,
    }
    struct CountingShutdown(Arc<AtomicUsize>);
    impl EngineShutdown for CountingShutdown {
        fn shutdown(&self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl EngineAssembler for CountingAssembler {
        fn assemble(
            &self,
            _db: &Db,
            _events: &broadcast::Sender<Event>,
            _pepper: &str,
            _data_dir: &std::path::Path,
            _bus: &Arc<CreationProgressBus>,
        ) -> Result<EngineAssembly, String> {
            self.assembled.fetch_add(1, Ordering::SeqCst);
            Ok(EngineAssembly::shutdown_only(Box::new(CountingShutdown(
                self.shutdowns.clone(),
            ))))
        }
    }

    /// An instance dir with a main DB (no tables needed for the state tests).
    fn make_instance() -> TempDir {
        let base = tempdir().unwrap();
        let data = base.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        // A writable open creates the encrypted main DB file.
        let _ = Writer::open_writable(&data.join("quilltap.db"), PEPPER).unwrap();
        base
    }

    fn config(base: &TempDir, env_pepper: Option<&str>) -> CoreConfig {
        CoreConfig {
            base_dir: base.path().to_path_buf(),
            version: "test".to_string(),
            env_pepper: env_pepper.map(str::to_string),
        }
    }

    #[tokio::test]
    async fn env_pepper_boot_is_operational_and_gates_work() {
        let base = make_instance();
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        match engine.dispatch(Request::Health).await {
            Response::Health(h) => {
                assert!(h.ready);
                assert_eq!(h.pepper_state, PepperState::NeedsVaultStorage);
                assert_eq!(h.status, "ok");
            }
            other => panic!("unexpected: {other:?}"),
        }
        // ListChats reaches the engine (the fixture has no chats table, so it
        // surfaces as an internal read error — NOT the locked refusal).
        match engine
            .dispatch(Request::ListChats {
                exclude_tag_ids: vec![],
                limit: None,
                include_autonomous: false,
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::Internal),
            other => panic!("unexpected: {other:?}"),
        }
        // ChatSend on a ready engine WITHOUT a driver (NoopAssembler): the
        // typed not-assembled refusal, not the locked one.
        match engine
            .dispatch(Request::ChatSend {
                chat_id: "c1".into(),
                content: "hi".into(),
                continue_mode: false,
                responding_participant_id: None,
                target_participant_ids: None,
                speaking_as_participant_id: None,
                file_ids: vec![],
                nudge: None,
                pending_tool_results: vec![],
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert_eq!(e.message, "chat dispatch not assembled");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn chat_send_gate_rejects_blank_message() {
        // v4 sendMessageSchema superRefine: a normal send with blank content, no
        // files, and no tool results → bad-request with the exact message.
        let base = make_instance();
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        match engine
            .dispatch(Request::ChatSend {
                chat_id: "c1".into(),
                content: "   ".into(),
                continue_mode: false,
                responding_participant_id: None,
                target_participant_ids: None,
                speaking_as_participant_id: None,
                file_ids: vec![],
                nudge: None,
                pending_tool_results: vec![],
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::BadRequest);
                assert_eq!(
                    e.message,
                    "Message must have content, attached files, or tool results"
                );
            }
            other => panic!("unexpected: {other:?}"),
        }
        // A continue-mode send with blank content is NOT rejected by the gate
        // (it reaches the driver → the not-assembled refusal on NoopAssembler).
        match engine
            .dispatch(Request::ChatSend {
                chat_id: "c1".into(),
                content: String::new(),
                continue_mode: true,
                responding_participant_id: None,
                target_participant_ids: None,
                speaking_as_participant_id: None,
                file_ids: vec![],
                nudge: Some(true),
                pending_tool_results: vec![],
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Internal);
                assert_eq!(e.message, "chat dispatch not assembled");
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn passphrase_vault_boots_locked_and_unlocks() {
        let base = make_instance();
        dbkey::save_dbkey(&base.path().join("data"), PEPPER, "open sesame").unwrap();

        let assembled = Arc::new(AtomicUsize::new(0));
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(CountingAssembler {
                assembled: assembled.clone(),
                shutdowns: shutdowns.clone(),
            }),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        // Locked boot: not assembled, ready-gated variants refused.
        assert_eq!(assembled.load(Ordering::SeqCst), 0);
        match engine
            .dispatch(Request::ListChats {
                exclude_tag_ids: vec![],
                limit: None,
                include_autonomous: false,
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::Locked);
                assert_eq!(e.pepper_state, Some(PepperState::NeedsPassphrase));
            }
            other => panic!("unexpected: {other:?}"),
        }
        match engine.dispatch(Request::UnlockState).await {
            Response::UnlockState(u) => {
                assert_eq!(u.state, PepperState::NeedsPassphrase);
                assert!(u.has_user_passphrase);
            }
            other => panic!("unexpected: {other:?}"),
        }

        // Wrong passphrase → bad request; still locked.
        match engine
            .dispatch(Request::Unlock {
                passphrase: "wrong".into(),
            })
            .await
        {
            Response::Error(e) => {
                assert_eq!(e.kind, ErrorKind::BadRequest);
                assert_eq!(e.message, "Invalid passphrase");
            }
            other => panic!("unexpected: {other:?}"),
        }

        // Right passphrase → assembled + resolved.
        match engine
            .dispatch(Request::Unlock {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::UnlockState(u) => {
                assert_eq!(u.state, PepperState::Resolved);
                assert!(u.has_user_passphrase);
            }
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(assembled.load(Ordering::SeqCst), 1);

        // Lock tears down; unlock re-assembles.
        match engine.dispatch(Request::Lock).await {
            Response::UnlockState(u) => assert_eq!(u.state, PepperState::NeedsPassphrase),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(shutdowns.load(Ordering::SeqCst), 1);
        assert!(engine.db().is_none());

        match engine
            .dispatch(Request::Unlock {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::UnlockState(u) => assert_eq!(u.state, PepperState::Resolved),
            other => panic!("unexpected: {other:?}"),
        }
        assert_eq!(assembled.load(Ordering::SeqCst), 2);
        assert!(engine.db().is_some());
    }

    #[tokio::test]
    async fn needs_setup_boot_refuses_unlock() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        match engine.dispatch(Request::Health).await {
            Response::Health(h) => {
                assert!(!h.ready);
                assert_eq!(h.pepper_state, PepperState::NeedsSetup);
            }
            other => panic!("unexpected: {other:?}"),
        }
        match engine
            .dispatch(Request::Unlock {
                passphrase: "anything".into(),
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::BadRequest),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn setup_provisions_a_bootable_instance() {
        // A truly empty data dir → needs-setup.
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let assembled = Arc::new(AtomicUsize::new(0));
        let shutdowns = Arc::new(AtomicUsize::new(0));
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(CountingAssembler {
                assembled: assembled.clone(),
                shutdowns: shutdowns.clone(),
            }),
            Arc::new(EmptyInstances),
        )
        .unwrap();

        // needs-setup boots locked; not assembled yet.
        assert_eq!(assembled.load(Ordering::SeqCst), 0);

        // Setup with a user passphrase mints a pepper, provisions, assembles.
        let pepper = match engine
            .dispatch(Request::Setup {
                passphrase: "open sesame".into(),
            })
            .await
        {
            Response::Setup(s) => {
                assert!(!s.pepper.is_empty());
                assert!(s.message.contains("will not be displayed again"));
                s.pepper
            }
            other => panic!("unexpected: {other:?}"),
        };
        assert_eq!(assembled.load(Ordering::SeqCst), 1);

        // The instance now boots and answers: ready, resolved, listChats -> [].
        match engine.dispatch(Request::Health).await {
            Response::Health(h) => {
                assert!(h.ready);
                assert_eq!(h.pepper_state, PepperState::Resolved);
            }
            other => panic!("unexpected: {other:?}"),
        }
        match engine
            .dispatch(Request::ListChats {
                exclude_tag_ids: vec![],
                limit: None,
                include_autonomous: false,
            })
            .await
        {
            Response::Chats(c) => assert!(c.is_empty()),
            other => panic!("unexpected: {other:?}"),
        }
        // The .dbkey was written and unlocks with the passphrase.
        let recovered = dbkey::load_pepper(&base.path().join("data"), Some("open sesame")).unwrap();
        assert_eq!(recovered, pepper);

        // A second Setup refuses (already set up).
        match engine
            .dispatch(Request::Setup {
                passphrase: "x".into(),
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::BadRequest),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn store_pepper_advances_needs_vault_storage_to_resolved() {
        // Env pepper + a main DB but no .dbkey → needs-vault-storage (operational).
        let base = make_instance();
        let engine = CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        match engine.dispatch(Request::UnlockState).await {
            Response::UnlockState(u) => assert_eq!(u.state, PepperState::NeedsVaultStorage),
            other => panic!("unexpected: {other:?}"),
        }

        // Store writes quilltap.dbkey and advances to resolved.
        match engine
            .dispatch(Request::StorePepper {
                passphrase: String::new(),
            })
            .await
        {
            Response::Ack(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        assert!(base.path().join("data").join("quilltap.dbkey").exists());
        match engine.dispatch(Request::UnlockState).await {
            Response::UnlockState(u) => {
                assert_eq!(u.state, PepperState::Resolved);
                assert!(!u.has_user_passphrase);
            }
            other => panic!("unexpected: {other:?}"),
        }
        // The written .dbkey decrypts to the same pepper (no user passphrase).
        assert_eq!(
            dbkey::load_pepper(&base.path().join("data"), None).unwrap(),
            PEPPER
        );
    }

    #[tokio::test]
    async fn change_passphrase_rewraps_and_gates_wrong_old() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        let engine = CoreEngine::boot(
            config(&base, None),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        )
        .unwrap();
        // Set up with an initial passphrase.
        match engine
            .dispatch(Request::Setup {
                passphrase: "first".into(),
            })
            .await
        {
            Response::Setup(_) => {}
            other => panic!("unexpected: {other:?}"),
        }

        // Wrong old passphrase → unauthorized.
        match engine
            .dispatch(Request::ChangePassphrase {
                old_passphrase: "wrong".into(),
                new_passphrase: "second".into(),
            })
            .await
        {
            Response::Error(e) => assert_eq!(e.kind, ErrorKind::Unauthorized),
            other => panic!("unexpected: {other:?}"),
        }

        // Correct old → re-wrapped; the new passphrase now unlocks, the old does not.
        match engine
            .dispatch(Request::ChangePassphrase {
                old_passphrase: "first".into(),
                new_passphrase: "second".into(),
            })
            .await
        {
            Response::Ack(_) => {}
            other => panic!("unexpected: {other:?}"),
        }
        let data = base.path().join("data");
        assert!(dbkey::load_pepper(&data, Some("first")).is_err());
        assert!(dbkey::load_pepper(&data, Some("second")).is_ok());
        // Both .dbkey files were written (v4 parity / cross-compat).
        assert!(data.join("quilltap.dbkey").exists());
        assert!(data.join("quilltap-llm-logs.dbkey").exists());
    }

    #[test]
    fn missing_main_db_refuses_boot() {
        let base = tempdir().unwrap();
        std::fs::create_dir_all(base.path().join("data")).unwrap();
        match CoreEngine::boot(
            config(&base, Some(PEPPER)),
            Box::new(NoopAssembler),
            Arc::new(EmptyInstances),
        ) {
            Err(BootError::MissingMainDb(_)) => {}
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("boot should have refused"),
        }
    }
}
