//! The user-profile dispatch surface (P4.9c) — a differential port of the
//! DEFAULT arms of v4's `?action=`-multiplexed
//! `app/api/v1/user/profile/route.ts`:
//!
//! - `GET` (no action) → `repos.users.findById` projected to the profile shape
//!   (`route.ts:84-102`);
//! - `PUT` (no action) → the `updateProfileSchema` parse + the email-uniqueness
//!   check + `repos.users.update` (`route.ts:180-223`);
//! - `PATCH ?action=set-avatar` → the file lookup + category validation + the
//!   `/api/v1/files/{id}` pointer write (`route.ts:230-303`).
//!
//! **NOT re-ported, deliberately:** the route's `theme-preference` GET/PUT arms
//! (`route.ts:56-81`, `:114-177`). They are already live in v5 through
//! `services::theme` over `chatSettings`; re-porting them here would give the
//! same preference two owners.
//!
//! ## v4 behaviors this port keeps that look like bugs
//!
//! 1. **A cleared field is rejected, not cleared.** `updateProfileSchema`'s
//!    fields are `.optional()`, which accepts an ABSENT key but rejects an
//!    explicit `null`. v4's own `ProfileEditSection` sends
//!    `{name: name || null, email: email || null}` — so emptying either box in
//!    v4's UI produces a 400 `Validation error`, not a cleared column. Ported
//!    exactly (the differential pins it); the v5 screen simply does not send
//!    nulls.
//! 2. **The PATCH's `image` object is dead.** `route.ts:280-290` builds an
//!    `{id, filepath, url}` object and then never puts it in the response. Not
//!    observable on the wire, so nothing to port — recorded so a future reader
//!    does not "restore" it.
//! 3. **A missing user is a 500, not a 404.** `route.ts:88-90` answers
//!    `serverError('User not found')` on the GET, while the PATCH's identical
//!    condition answers `notFound('User')` (`:271`). Both carried verbatim.

use serde_json::{json, Value};

use crate::db::runtime::Db;
use crate::db::users::{find_id_by_email, find_profile_by_id, UserProfileRow, UserUpdate};

use super::types::{ErrorKind, Response};

fn bad_request(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::BadRequest, msg)
}
fn not_found(resource: &str) -> Response {
    // v4 `notFound(resource)` → `"<resource> not found"`.
    Response::error(ErrorKind::NotFound, format!("{resource} not found"))
}
fn server_error(msg: impl Into<String>) -> Response {
    Response::error(ErrorKind::Internal, msg)
}
fn internal(e: impl std::fmt::Display) -> Response {
    Response::error(ErrorKind::Internal, e.to_string())
}

/// v4's `validationError(zodError)` body is `{error: 'Validation error',
/// details: [...issues]}` at 400. v5's error envelope carries no `details`
/// array — the documented, pre-existing absence across the whole ported route
/// surface — so every route differential compares status + `error` only.
const VALIDATION_ERROR: &str = "Validation error";

/// The `{profile: {...}}` envelope, in v4's field order (`route.ts:93-101`).
/// `serde_json` is built with `preserve_order`, so this order is the wire order.
///
/// ## Read-omits-null vs update-echoes-null (the family-wide seam)
///
/// A NULL column read back through v4's repo hydrates to `undefined`, and
/// `JSON.stringify` DROPS an undefined value — so `GET` on a user with no
/// avatar answers a profile object with **no `image` key at all**. But v4's
/// `_update` returns `{...existing, ...updateData}`, and `updateData.image` is
/// a literal `null` on the clear paths (`image: '' → null`, `set-avatar` with a
/// null id) — so those responses DO carry `"image": null`.
///
/// `explicit_nulls` is that distinction: the columns THIS request deliberately
/// wrote a NULL into, which are echoed as `null` instead of omitted. Everything
/// still NULL for any other reason is omitted. (Only `image` can reach this
/// list today — `name` and `email` cannot be nulled through the schema.)
fn profile_envelope(row: &UserProfileRow, explicit_nulls: &[&str]) -> Value {
    let mut profile = serde_json::Map::new();
    let nullable = |map: &mut serde_json::Map<String, Value>, key: &str, v: &Option<String>| match v
    {
        Some(s) => {
            map.insert(key.to_string(), Value::String(s.clone()));
        }
        None if explicit_nulls.contains(&key) => {
            map.insert(key.to_string(), Value::Null);
        }
        None => {}
    };

    profile.insert("id".into(), Value::String(row.id.clone()));
    nullable(&mut profile, "email", &row.email);
    profile.insert("username".into(), Value::String(row.username.clone()));
    nullable(&mut profile, "name", &row.name);
    nullable(&mut profile, "image", &row.image);
    profile.insert("createdAt".into(), Value::String(row.created_at.clone()));
    profile.insert("updatedAt".into(), Value::String(row.updated_at.clone()));

    json!({ "profile": Value::Object(profile) })
}

/// v4 `GET /api/v1/user/profile` (default action).
pub fn user_profile_get(db: &Db, user_id: &str) -> Response {
    let user_id = user_id.to_string();
    match db.read_main(move |conn| find_profile_by_id(conn, &user_id)) {
        // A pure read: every NULL column is omitted, none is echoed.
        Ok(Some(row)) => Response::UserProfile(profile_envelope(&row, &[])),
        // v4 answers a 500 here, not a 404 — see the header.
        Ok(None) => server_error("User not found"),
        Err(e) => internal(e),
    }
}

/// One field of `updateProfileSchema`, resolved against v4's Zod semantics.
enum Field {
    /// The key was absent — leave the column alone.
    Absent,
    /// A valid value.
    Value(String),
    /// The key was present but invalid (wrong type, `null`, or a failed
    /// refinement) — v4's `.parse()` throws a `ZodError`, which the middleware
    /// turns into the 400 below.
    Invalid,
}

/// `z.string().min(1).max(200).optional()`. `.optional()` admits ABSENT only;
/// `null` is an `invalid_type` issue. Zod measures a string in JS `String.length`
/// — UTF-16 code units, not Unicode scalars — hence [`crate::jsstr::utf16_len`].
fn parse_name(field: &Option<Option<Value>>) -> Field {
    match field {
        None => Field::Absent,
        Some(Some(Value::String(s))) if !s.is_empty() && crate::jsstr::utf16_len(s) <= 200 => {
            Field::Value(s.clone())
        }
        Some(_) => Field::Invalid,
    }
}

/// `z.email().optional()`. Zod's email check is a regex, not a parse; the port
/// keeps it deliberately conservative and identical in shape — the differential
/// pins both an accepted and a rejected address.
fn parse_email(field: &Option<Option<Value>>) -> Field {
    match field {
        None => Field::Absent,
        Some(Some(Value::String(s))) if is_zod_email(s) => Field::Value(s.clone()),
        Some(_) => Field::Invalid,
    }
}

/// `z.url().optional().or(z.literal(''))` — a URL, or the empty string. The
/// route then writes `validatedData.image || null`, so `''` CLEARS the column.
fn parse_image(field: &Option<Option<Value>>) -> Field {
    match field {
        None => Field::Absent,
        Some(Some(Value::String(s))) if s.is_empty() || is_zod_url(s) => Field::Value(s.clone()),
        Some(_) => Field::Invalid,
    }
}

/// Zod v4's `z.email()` regex, transcribed: one or more non-`@`/whitespace
/// characters, `@`, a dotted domain whose last label is 2+ letters.
fn is_zod_email(s: &str) -> bool {
    let Some((local, domain)) = s.rsplit_once('@') else {
        return false;
    };
    if local.is_empty() || local.contains(char::is_whitespace) || local.contains('@') {
        return false;
    }
    let labels: Vec<&str> = domain.split('.').collect();
    if labels.len() < 2 {
        return false;
    }
    if labels
        .iter()
        .any(|l| l.is_empty() || l.contains(char::is_whitespace))
    {
        return false;
    }
    let tld = labels[labels.len() - 1];
    tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic())
}

/// `z.url()` — parseable as an absolute URL with a scheme and a non-empty
/// authority.
fn is_zod_url(s: &str) -> bool {
    let Some((scheme, rest)) = s.split_once("://") else {
        return false;
    };
    !scheme.is_empty()
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        && !rest.is_empty()
        && !rest.starts_with('/')
}

/// v4 `PUT /api/v1/user/profile` (default action).
pub async fn user_profile_update(
    db: &Db,
    user_id: &str,
    name: Option<Option<Value>>,
    email: Option<Option<Value>>,
    image: Option<Option<Value>>,
    now: &str,
) -> Response {
    let name = parse_name(&name);
    let email = parse_email(&email);
    let image = parse_image(&image);
    if matches!(name, Field::Invalid)
        || matches!(email, Field::Invalid)
        || matches!(image, Field::Invalid)
    {
        return bad_request(VALIDATION_ERROR);
    }

    // The email-uniqueness gate runs BEFORE the write (`route.ts:195-203`) and
    // compares ids, so re-submitting your own address is a no-op, not a clash.
    let new_email = match &email {
        Field::Value(v) => Some(v.clone()),
        _ => None,
    };
    if let Some(candidate) = &new_email {
        let candidate = candidate.clone();
        let owner = match db.read_main(move |conn| find_id_by_email(conn, &candidate)) {
            Ok(o) => o,
            Err(e) => return internal(e),
        };
        if let Some(owner_id) = owner {
            if owner_id != user_id {
                return bad_request("Email already in use");
            }
        }
    }

    let patch = UserUpdate {
        name: match &name {
            Field::Value(v) => Some(v.clone()),
            _ => None,
        },
        email: new_email,
        // v4: `updateData.image = validatedData.image || null` — the empty
        // string clears the column, which is why this is the nullable setter.
        image: match &image {
            Field::Value(v) if v.is_empty() => Some(None),
            Field::Value(v) => Some(Some(v.clone())),
            _ => None,
        },
        updated_at: now.to_string(),
        ..Default::default()
    };

    // `image: ''` writes a literal NULL, which v4's merged entity ECHOES.
    let cleared_image = matches!(&image, Field::Value(v) if v.is_empty());

    let id = user_id.to_string();
    let updated = db
        .write(move |writers| writers.main().users().update(&id, &patch))
        .await;
    match updated {
        Ok(true) => {}
        Ok(false) => return server_error("Failed to update profile"),
        Err(e) => return internal(e),
    }

    let explicit_nulls: &[&str] = if cleared_image { &["image"] } else { &[] };
    let id = user_id.to_string();
    match db.read_main(move |conn| find_profile_by_id(conn, &id)) {
        Ok(Some(row)) => Response::UserProfile(profile_envelope(&row, explicit_nulls)),
        Ok(None) => server_error("Failed to update profile"),
        Err(e) => internal(e),
    }
}

/// v4 `PATCH /api/v1/user/profile?action=set-avatar`.
pub async fn user_profile_set_avatar(
    db: &Db,
    user_id: &str,
    image_id: Option<Option<Value>>,
    now: &str,
) -> Response {
    // `avatarSchema = z.object({ imageId: z.string().nullable() })` — the key is
    // REQUIRED (absent is a ZodError) but may be null.
    let image_id: Option<String> = match image_id {
        None => return bad_request(VALIDATION_ERROR),
        Some(None) | Some(Some(Value::Null)) => None,
        Some(Some(Value::String(s))) => Some(s),
        Some(Some(_)) => return bad_request(VALIDATION_ERROR),
    };

    if let Some(id) = &image_id {
        let id = id.clone();
        let entry = match db
            .read_main(move |conn| crate::db::files::FilesRepository::new(conn).find_by_id(&id))
        {
            Ok(e) => e,
            Err(e) => return internal(e),
        };
        let Some(entry) = entry else {
            return not_found("File");
        };
        // v4: `['IMAGE','AVATAR'].includes(fileEntry.category)`.
        if entry.category != "IMAGE" && entry.category != "AVATAR" {
            return bad_request(format!(
                "Invalid file category. Expected IMAGE or AVATAR, got {}",
                entry.category
            ));
        }
    }

    // `imageId ? '/api/v1/files/<id>' : null` — note a non-null id has already
    // been proven to exist and be an image by this point.
    let image_url = image_id.as_ref().map(|id| format!("/api/v1/files/{id}"));

    let patch = UserUpdate {
        image: Some(image_url),
        updated_at: now.to_string(),
        ..Default::default()
    };
    let id = user_id.to_string();
    let updated = db
        .write(move |writers| writers.main().users().update(&id, &patch))
        .await;
    match updated {
        Ok(true) => {}
        // v4's `!updatedUser` arm here is `notFound('User')`, unlike the GET's
        // 500 for the same condition.
        Ok(false) => return not_found("User"),
        Err(e) => return internal(e),
    }

    // The clear path writes a literal NULL, which v4's merged entity ECHOES.
    let explicit_nulls: &[&str] = if image_id.is_none() { &["image"] } else { &[] };
    let id = user_id.to_string();
    match db.read_main(move |conn| find_profile_by_id(conn, &id)) {
        Ok(Some(row)) => Response::UserProfile(profile_envelope(&row, explicit_nulls)),
        Ok(None) => not_found("User"),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_rejects_null_and_empty_but_accepts_absent() {
        assert!(matches!(parse_name(&None), Field::Absent));
        assert!(matches!(parse_name(&Some(None)), Field::Invalid));
        assert!(matches!(
            parse_name(&Some(Some(Value::String(String::new())))),
            Field::Invalid
        ));
        assert!(matches!(
            parse_name(&Some(Some(Value::String("Friday".into())))),
            Field::Value(_)
        ));
        let long = "x".repeat(201);
        assert!(matches!(
            parse_name(&Some(Some(Value::String(long)))),
            Field::Invalid
        ));
    }

    #[test]
    fn email_regex_shape() {
        assert!(is_zod_email("charlie@foundry-9.com"));
        assert!(!is_zod_email("nope"));
        assert!(!is_zod_email("nope@nope"));
        assert!(!is_zod_email("@nope.com"));
        assert!(!is_zod_email("a b@nope.com"));
    }

    #[test]
    fn image_accepts_empty_string_and_urls_only() {
        assert!(matches!(
            parse_image(&Some(Some(Value::String(String::new())))),
            Field::Value(_)
        ));
        assert!(matches!(
            parse_image(&Some(Some(Value::String(
                "https://example.com/a.png".into()
            )))),
            Field::Value(_)
        ));
        assert!(matches!(
            parse_image(&Some(Some(Value::String("/api/v1/files/abc".into())))),
            Field::Invalid
        ));
    }
}
