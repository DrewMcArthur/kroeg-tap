use jsonld::nodemap::Pointer;
use std::error::Error;
use std::fmt;

use kroeg_tap::{as2, Context, MessageHandler};

#[derive(Debug)]
pub enum ServerCreateError {
    FailedToRetrieve,
    MissingObject,
}

impl fmt::Display for ServerCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServerCreateError::FailedToRetrieve => write!(f, "Failed to retrieve object. Timeout?"),
            ServerCreateError::MissingObject => write!(
                f,
                "The as:object predicate is missing or occurs more than once"
            ),
        }
    }
}

impl Error for ServerCreateError {}

pub struct ServerCreateHandler;

#[async_trait::async_trait]
impl MessageHandler for ServerCreateHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let root = match context.entity_store.get(elem.to_owned(), false).await? {
            Some(root) => root,
            None => return Ok(()),
        };

        if !root.main().types.iter().any(|f| f == as2!(Create)) {
            return Ok(());
        }

        let inbox = context
            .entity_store
            .get(inbox.to_owned(), true)
            .await?
            .unwrap();
        let attributed_to = &inbox.main()[as2!(attributedTo)];

        if attributed_to.is_empty() {
            return Ok(());
        }

        for pointer in &root.main()[as2!(object)] {
            if let Pointer::Id(id) = pointer {
                let object = context
                    .entity_store
                    .get(id.to_owned(), false)
                    .await?
                    .ok_or(ServerCreateError::FailedToRetrieve)?;

                for item in &object.main()[as2!(inReplyTo)] {
                    let id = if let Pointer::Id(id) = item {
                        id.to_owned()
                    } else {
                        continue;
                    };

                    if let Some(replied) = context.entity_store.get(id, true).await? {
                        if !replied.is_owned(&context) {
                            continue;
                        }

                        // Ensure replies only get processed iff the user themselves gets the activity.
                        if &replied.main()[as2!(attributedTo)] != attributed_to {
                            continue;
                        }

                        if let [Pointer::Id(id)] = &replied.main()[as2!(replies)] as &[Pointer] {
                            context
                                .entity_store
                                .insert_collection(id.to_owned(), object.id().to_owned())
                                .await?;
                        }
                    }
                }
            } else {
                return Err(ServerCreateError::MissingObject.into());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::ServerCreateHandler;
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use async_std::task::block_on;
    use kroeg_tap::{as2, MessageHandler};

    fn setup() -> (TestStore, ()) {
        (
            TestStore::new(vec![
                object_under_test!(remote "/like" => {
                    types => [as2!(Like)];
                    as2!(object) => ["/object"];
                }),
                object_under_test!(local "/inbox" => {
                    types => [as2!(OrderedCollection)];
                    as2!(attributedTo) => ["/actor"];
                }),
                object_under_test!(local "/local" => {
                    types => [as2!(Note)];
                    as2!(replies) => ["/replies"];
                    as2!(attributedTo) => ["/actor"];
                }),
                object_under_test!(local "/local/create" => {
                    types => [as2!(Create)];
                    as2!(object) => ["/local/object"];
                }),
                object_under_test!(local "/local/object" => {
                    types => [as2!(Note)];
                    as2!(inReplyTo) => ["/local"];
                }),
            ]),
            (),
        )
    }

    #[test]
    fn ignores_non_create() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ServerCreateHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/like".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read("/like"),
                    "Handler did not read the root object"
                );
                assert!(
                    !store.has_read("/a/object"),
                    "Handler attempted to process the Like"
                );
            }
            Err(e) => panic!("handler returned error: {}", e),
        }
    }

    #[test]
    fn handles_local_object() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ServerCreateHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/local/create".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read_all(&["/local/create", "/local/object", "/local"]),
                    "Handler did not read all objects"
                );
                assert!(
                    store.contains("/replies", "/local/object"),
                    "Reply has not been recorded properly"
                );
            }
            Err(e) => panic!("handler returned error: {}", e),
        }
    }
}
