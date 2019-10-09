use jsonld::nodemap::Pointer;
use std::error::Error;

use kroeg_tap::{as2, Context, MessageHandler};

pub struct ServerLikeHandler;

#[async_trait::async_trait]
impl MessageHandler for ServerLikeHandler {
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

        let is_like = root.main().types.iter().any(|f| f == as2!(Like));
        let is_announce = root.main().types.iter().any(|f| f == as2!(Announce));

        if !is_like && !is_announce {
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
            let id = if let Pointer::Id(id) = pointer {
                id.to_owned()
            } else {
                continue;
            };
            if let Some(object) = context.entity_store.get(id, false).await? {
                if !object.is_owned(&context) {
                    continue;
                }

                // Ensure that likes only apply iff received by the user themselves.
                if &object.main()[as2!(attributedTo)] != attributed_to {
                    continue;
                }

                if is_like {
                    if let [Pointer::Id(collection)] = &object.main()[as2!(likes)] as &[Pointer] {
                        context
                            .entity_store
                            .insert_collection(collection.to_owned(), elem.to_owned())
                            .await?;
                    }
                }

                if is_announce {
                    if let [Pointer::Id(collection)] = &object.main()[as2!(shares)] as &[Pointer] {
                        context
                            .entity_store
                            .insert_collection(collection.to_owned(), elem.to_owned())
                            .await?;
                    }
                }
            }
        }

        return Ok(());
    }
}

#[cfg(test)]
mod test {
    use super::ServerLikeHandler;
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use async_std::task::block_on;
    use kroeg_tap::{as2, MessageHandler};

    fn setup() -> (TestStore, ()) {
        (
            TestStore::new(vec![
                object_under_test!(remote "/like_a" => {
                    types => [as2!(Like)];
                    as2!(object) => ["/object_a"];
                }),
                object_under_test!(remote "/like_b" => {
                    types => [as2!(Like)];
                    as2!(object) => ["/object_b"];
                }),
                object_under_test!(remote "/like_c" => {
                    types => [as2!(Announce)];
                    as2!(object) => ["/object_a"];
                }),
                object_under_test!(local "/inbox" => {
                    types => [as2!(OrderedCollection)];
                    as2!(attributedTo) => ["/actor"];
                }),
                object_under_test!(local "/object_a" => {
                    types => [as2!(Note)];
                    as2!(likes) => ["/object_a/likes"];
                    as2!(shares) => ["/object_a/shares"];
                    as2!(attributedTo) => ["/actor"];
                }),
                object_under_test!(remote "/object_b" => {
                    types => [as2!(Note)];
                    as2!(likes) => ["/object_b/likes"];
                    as2!(shares) => ["/object_b/shares"];
                    as2!(attributedTo) => ["/actor"];
                }),
            ]),
            (),
        )
    }

    #[test]
    fn handles_base_case() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ServerLikeHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/like_a".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read_all(&["/like_a", "/object_a"]),
                    "Handler did not read all the expected objects"
                );
                assert!(
                    store.contains("/object_a/likes", "/like_a"),
                    "Handler did not register like"
                );
            }
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn handles_remote_like() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ServerLikeHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/like_b".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read_all(&["/like_b", "/object_b"]),
                    "Handler did not read all the expected objects"
                );
                assert!(
                    !store.contains("/object_b/likes", "/like_b"),
                    "Handler registered like on remote object"
                );
            }
            Err(e) => panic!("Error: {}", e),
        }
    }

    #[test]
    fn applies_to_announce_too() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ServerLikeHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/like_c".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read_all(&["/like_c", "/object_a"]),
                    "Handler did not read the necessary objects"
                );
                assert!(
                    store.contains("/object_a/shares", "/like_c"),
                    "Handler did not rgister Announce"
                );
            }
            Err(e) => panic!("Error: {}", e),
        }
    }
}
