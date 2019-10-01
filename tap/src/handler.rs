use crate::user::Context;

use std::error::Error;

/// Handler used to process incoming ActivityPub messages.
#[async_trait::async_trait]
pub trait MessageHandler: Send + Sync {
    /// Process a single message.
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        inbox: &mut String,
        id: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>>;
}
