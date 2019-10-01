use jsonld::nodemap::Pointer;
use std::error::Error;

use kroeg_tap::{as2, Context, MessageHandler};

pub struct ServerLikeHandler;

#[async_trait::async_trait]
impl MessageHandler for ServerLikeHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        _inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let root = match context.entity_store.get(elem.to_owned(), false).await? {
            Some(root) => root,
            None => return Ok(()),
        };

        if !root.main().types.iter().any(|f| f == as2!(Like)) {
            return Ok(());
        }

        for pointer in &root.main()[as2!(object)] {
            let id = if let Pointer::Id(id) = pointer {
                id.to_owned()
            } else {
                continue;
            };
            if let Some(liked) = context.entity_store.get(id, false).await? {
                if !liked.is_owned(&context) {
                    continue;
                }

                if let [Pointer::Id(collection)] = &liked.main()[as2!(likes)] as &[Pointer] {
                    context
                        .entity_store
                        .insert_collection(collection.to_owned(), elem.to_owned())
                        .await?;
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
                object_under_test!(local "/object_a" => {
                    types => [as2!(Note)];
                    as2!(likes) => ["/object_a/likes"];
                }),
                object_under_test!(remote "/object_b" => {
                    types => [as2!(Note)];
                    as2!(likes) => ["/object_b/likes"];
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
    fn only_applies_to_likes() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(ServerLikeHandler.handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/like_c".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read("/like_c"),
                    "Handler did not read the incoming object"
                );
                assert!(
                    !store.has_read("/object_a"),
                    "Handler read the target of an Announce"
                );
            }
            Err(e) => panic!("Error: {}", e),
        }
    }
}
