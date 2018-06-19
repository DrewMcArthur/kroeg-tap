use super::entitystore::EntityStore;
use super::user::Context;

use futures::prelude::*;
use std::error::Error;

/// Handler used to process incoming ActivityPub messages.
pub trait MessageHandler<T: EntityStore> {
    type Error: Error + Send + Sync + 'static;

    /// The `Future` that gets fullfilled by this `MessageHandler`.
    ///
    /// On success, the handler returns the `Context` and `EntityStore`,
    /// any error will be bubbled back to the user.
    type Future: Future<Item = (Context, T), Error = Self::Error> + Send;

    /// Process a single message, consuming the message handler.
    fn handle(self, context: Context, entitystore: T, inbox: String, id: String) -> Self::Future;
}

/// Macro for translating short as:Note style IDs to full strings as used in
/// `Entity`. e.g. `as2!(name)`.
#[macro_export]
macro_rules! as2 {
    ($ident:ident) => {
        concat!("https://www.w3.org/ns/activitystreams#", stringify!($ident))
    };
}
