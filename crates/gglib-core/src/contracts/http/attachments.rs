//! Where an image a message carries is sent and read.
//!
//! Three doors: the daemon's own store, the paired machine's through the
//! tunnel, and the proxy's for a paired device. One id names an image on
//! all three: the SHA-256 of its bytes, in lowercase hex.

/// The `Cache-Control` of this machine's own stored image, read by its id.
///
/// At the daemon's door: this machine's page may keep it for good. The id is
/// the hash of the bytes, so what an id names never changes.
pub const ATTACHMENT_CACHE_CONTROL: &str = "private, max-age=31536000, immutable";

/// The `Cache-Control` of a stored image read across a pairing.
///
/// At the proxy's door, which a paired device reads, and at the joined
/// machine's relay, which shows the far machine's image on this machine's
/// page. No side keeps the other's chats, images included, so forgetting the
/// pairing takes everything back.
pub const PAIRED_ATTACHMENT_CACHE_CONTROL: &str = "no-store";

/// What a read is told when its path is not an id, with `404
/// attachment_not_found`, at every door: such a path names no image.
pub const NOT_AN_ATTACHMENT_ID: &str = "No stored image has that id.";

/// The daemon's store: `POST` takes an image as the raw body and answers
/// what was stored.
pub const ATTACHMENTS_PATH: &str = "/api/attachments";

/// One stored image, interpolating `id` into [`ATTACHMENTS_PATH`]: `GET`
/// answers its bytes.
#[must_use]
pub fn attachment_path(id: &str) -> String {
    format!("{ATTACHMENTS_PATH}/{id}")
}

/// The paired machine's store, through the tunnel: `POST`, as
/// [`ATTACHMENTS_PATH`].
pub const REMOTE_ATTACHMENTS_PATH: &str = "/api/remote/attachments";

/// One image the paired machine stores, interpolating `id` into
/// [`REMOTE_ATTACHMENTS_PATH`]: `GET`.
#[must_use]
pub fn remote_attachment_path(id: &str) -> String {
    format!("{REMOTE_ATTACHMENTS_PATH}/{id}")
}

/// The two routes to the paired machine's store, each with its verb and the
/// id instantiated: part of [`super::daemon::remote_route_contract`]'s sweep.
#[must_use]
pub fn remote_route_contract() -> [(&'static [&'static str], String); 2] {
    [
        (&["POST"], REMOTE_ATTACHMENTS_PATH.to_owned()),
        (&["GET"], remote_attachment_path(&"0".repeat(64))),
    ]
}

/// The proxy's door for a paired device, under its `/v1`: `POST` here, and
/// `GET` with `/{id}` after it.
pub const PROXY_ATTACHMENTS_PATH: &str = "/attachments";
