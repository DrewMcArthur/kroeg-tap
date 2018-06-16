use std::collections::HashMap;

/// Context for an ActivityPub request.
#[derive(Debug, Clone)]
pub struct Context {
    /// User data for this request.
    pub user: User,

    /// The base URI of the server, e.g. `https://example.com`
    pub server_base: String,

    /// Instance ID, allows for multiple servers to share a database.
    pub instance_id: u32,
}

/// The authorization data for a single request.
#[derive(Debug, Clone)]
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

    /// An opaque token used for revoking tokens.
    pub token_identifier: String,
}
