use super::entitystore::EntityStore;
use super::user::Context;

use futures::prelude::*;
use std::error::Error;

/// Handler used to process incoming ActivityPub messages.
pub trait MessageHandler<T: EntityStore>: Send {
    /// Process a single message.
    fn handle(
        &self,
        context: Context,
        entitystore: T,
        inbox: String,
        id: String,
    ) -> Box<Future<Item = (Context, T, String), Error = Box<Error + Send + Sync + 'static>> + Send>;
}
