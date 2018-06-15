use chrono::offset::Utc;
use chrono::DateTime;
use std::collections::HashMap;

/// Context for an ActivityPub request.
pub struct Context {
    /// User data for this request.
    pub user: User,

    /// The base URI of the server, e.g. `https://example.com`
    pub server_base: String,
}

/// The authorization data for a single request.
pub struct User {
    /// A list of unstructured claims for this
    /// token and user.
    pub claims: HashMap<String, String>,

    /// The issuer of this token, if any.
    pub issuer: Option<String>,

    /// The user ID this token is talking about.
    pub subject: String,

    /// The list of servers this token is meant for.
    pub audience: Vec<String>,

    /// Date that this token expires.
    pub expiration: DateTime<Utc>,

    /// Date when this token becomes valid.
    pub not_before: DateTime<Utc>,

    /// Date when the token was created.
    pub issued_at: DateTime<Utc>,

    /// An opaque token used for revoking tokens.
    pub token_identifier: String,
}
