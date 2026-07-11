//! A small `multipart/form-data` helper mirroring the browser `FormData`
//! semantics the v4 routes rely on: whole-body buffering, string-or-file fields,
//! `get` (first value) and `getAll` (every value). This is the reusable edge
//! machinery for quilltap-web's first multipart routes (the characters photo
//! upload + the ST-card import); every OTHER v4 multipart route (images-v2,
//! chat attachments, mount ingest, `.qtap`/theme install) is a documented
//! deferral — the helper is built to serve them when their families land.
//!
//! v4 buffers the whole body (`Buffer.from(await file.arrayBuffer())`); so do we
//! (no streaming uploads). A part is a "file" (`instanceof File` in v4) exactly
//! when its `Content-Disposition` carries a `filename` — axum surfaces that as
//! `field.file_name()`.

use axum::extract::{FromRequest, Multipart, Request};

/// One parsed multipart field (buffered).
pub struct Field {
    pub name: String,
    /// `Some` ⟺ v4's `instanceof File` (the part carried a `filename`).
    pub filename: Option<String>,
    pub content_type: Option<String>,
    pub bytes: Vec<u8>,
}

/// The parsed form — field order preserved (v4's `getAll` is insertion-ordered).
pub struct FormData {
    fields: Vec<Field>,
}

impl FormData {
    /// Parse a request body as `multipart/form-data`, buffering every field.
    /// Mirrors v4's `req.formData().catch(() => null)` — a malformed body yields
    /// `Err`, which the caller maps to the 400 v4 returns.
    pub async fn from_request<S>(req: Request, state: &S) -> Result<FormData, String>
    where
        S: Send + Sync,
    {
        let mut multipart = Multipart::from_request(req, state)
            .await
            .map_err(|e| e.to_string())?;
        let mut fields = Vec::new();
        while let Some(field) = multipart.next_field().await.map_err(|e| e.to_string())? {
            let name = field.name().unwrap_or("").to_string();
            let filename = field.file_name().map(str::to_string);
            let content_type = field.content_type().map(str::to_string);
            let bytes = field.bytes().await.map_err(|e| e.to_string())?.to_vec();
            fields.push(Field {
                name,
                filename,
                content_type,
                bytes,
            });
        }
        Ok(FormData { fields })
    }

    /// The first field with this name that is a FILE (`formData.get(name)` where
    /// the caller then checks `instanceof File`).
    pub fn file(&self, name: &str) -> Option<&Field> {
        self.fields
            .iter()
            .find(|f| f.name == name && f.filename.is_some())
    }

    /// The first field with this name, decoded as UTF-8 text — v4's
    /// `formData.get(name) as string`. `None` when absent.
    pub fn text(&self, name: &str) -> Option<String> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| String::from_utf8_lossy(&f.bytes).into_owned())
    }

    /// Every string value for this name, non-empty — v4's
    /// `formData.getAll(name).filter(t => typeof t === 'string' && t.length > 0)`
    /// (file-typed parts are dropped, matching `typeof File !== 'string'`).
    pub fn all_text(&self, name: &str) -> Vec<String> {
        self.fields
            .iter()
            .filter(|f| f.name == name && f.filename.is_none())
            .map(|f| String::from_utf8_lossy(&f.bytes).into_owned())
            .filter(|s| !s.is_empty())
            .collect()
    }
}
