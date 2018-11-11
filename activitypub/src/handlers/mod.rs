// Both C2S and S2S, verifies base constraints
mod verify_required;
pub use self::verify_required::*;

// C2S, handles {type: Create, object: {type: Person}}
mod create_actor;
pub use self::create_actor::*;

// C2S, handles objects that aren't activities.
mod auto_create;
pub use self::auto_create::*;

// C2S, adds likes/shares/replies collections to objects that are in a Create.
mod client_create;
pub use self::client_create::*;

// C2S, adds liked objects to their `liked` collection.
mod client_like;
pub use self::client_like::*;

// C2S, undoes Like/Follow
mod client_undo;
pub use self::client_undo::*;

// S2S, adds reply to owned inReplyTo objects
mod server_create;
pub use self::server_create::*;
