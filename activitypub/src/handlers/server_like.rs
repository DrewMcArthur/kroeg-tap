use futures::{
    future::{self, Either},
    stream, Future, Stream,
};
use jsonld::nodemap::Pointer;
use kroeg_tap::{box_store_error, Context, EntityStore, MessageHandler};
use std::error::Error;

pub struct ServerLikeHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ServerLikeHandler {
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Box<
        Future<Item = (Context, T, String), Error = (Box<Error + Send + Sync + 'static>, T)> + Send,
    > {
        Box::new(
            store
                .get(elem.to_owned(), false)
                .map(|(elem, store)| match elem {
                    Some(elem) if elem.main().types.contains(&as2!(Like).to_owned()) => (
                        elem.main()[as2!(object)]
                            .iter()
                            .filter_map(|f| match f {
                                Pointer::Id(id) => Some(id.to_owned()),
                                _ => None,
                            })
                            .collect(),
                        store,
                    ),

                    _ => (vec![], store),
                })
                .and_then(move |(items, store)| {
                    stream::iter_ok(items).fold(
                        (context, store, elem),
                        |(context, store, elem), item| {
                            store.get(item, true).and_then(move |(item, store)| {
                                let item = match item {
                                    Some(item) if item.is_owned(&context) => item,
                                    _ => return Either::A(future::ok((context, store, elem))),
                                };

                                match item.main()[as2!(likes)].get(0).cloned() {
                                    Some(Pointer::Id(id)) => Either::B(
                                        store
                                            .insert_collection(id, elem.to_owned())
                                            .map(|store| (context, store, elem)),
                                    ),
                                    _ => Either::A(future::ok((context, store, elem))),
                                }
                            })
                        },
                    )
                })
                .map_err(box_store_error),
        )
    }
}

#[cfg(test)]
mod test {
    use super::ServerLikeHandler;
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use futures::Async;
    use jsonld::nodemap::Entity;
    use kroeg_tap::{Context, MessageHandler};

    fn setup() -> (Context, TestStore) {
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
            object_under_test!(local "/object_a/likes" => {
                types => [as2!(OrderedCollection)];
            }),
        ])
    }

    #[test]
    fn handles_base_case() {
        let (context, store) = setup();

        if let Async::Ready((context, store, elem)) = ServerLikeHandler
            .handle(context, store, "/inbox".to_owned(), "/like_a".to_owned())
            .poll()
            .unwrap()
        {
            store.assert_all_read(&["/like_a", "/object_a"]);
            store.assert_contains("/object_a/likes", "/like_a");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn handles_remote_like() {
        let (context, store) = setup();

        if let Async::Ready((context, store, elem)) = ServerLikeHandler
            .handle(context, store, "/inbox".to_owned(), "/like_b".to_owned())
            .poll()
            .unwrap()
        {
            store.assert_all_read(&["/like_b", "/object_b"]);
            store.assert_does_not_contain("/object_b/likes", "/like_b");
        } else {
            unreachable!();
        }
    }

    #[test]
    fn only_applies_to_likes() {
        let (context, store) = setup();

        if let Async::Ready((context, store, elem)) = ServerLikeHandler
            .handle(context, store, "/inbox".to_owned(), "/like_c".to_owned())
            .poll()
            .unwrap()
        {
            store.assert_read("/like_c");
            store.assert_unread("/object_a");
        } else {
            unreachable!();
        }
    }
}
