// Shared handler between outbox, inbox, and sharedInbox.
mod verify_required;
pub use self::verify_required::*;

// --- Outbox only: ---

// Handles wrapping non-activities with a Create activity.
mod auto_create;
pub use self::auto_create::*;

// Handles creating an actor.
mod create_actor;
pub use self::create_actor::*;

// Adds likes/shares/replies collections to `Create`d objects..
mod client_create;
pub use self::client_create::*;

// Adds liked objects to their `liked` collection.
mod client_like;
pub use self::client_like::*;

// Undoes Like/Follow/Accept
mod client_undo;
pub use self::client_undo::*;

// --- Inbox only: ---

// Adds object to replies if inReplyTo is an owned object.
mod server_create;
pub use self::server_create::*;

// Adds object to likes and/or shares if object being liked/announced is owned.
mod server_like;
pub use self::server_like::*;

// Handles follows, and their accept/rejects.
mod server_follow;
pub use self::server_follow::*;
