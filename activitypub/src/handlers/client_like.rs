use futures::{
    future::{self, Either},
    stream, Future, Stream,
};
use jsonld::nodemap::Pointer;
use kroeg_tap::{box_store_error, Context, EntityStore, MessageHandler};
use std::error::Error;

pub struct ClientLikeHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ClientLikeHandler {
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Box<
        Future<Item = (Context, T, String), Error = (Box<Error + Send + Sync + 'static>, T)> + Send,
    > {
        let subject = context.user.subject.to_owned();
        let root = elem.to_owned();

        Box::new(
            store
                .get(elem, true)
                .and_then(|(elem, store)| match elem {
                    Some(elem) if elem.main().types.contains(&as2!(Like).to_owned()) => {
                        Either::A(store.get(subject, true).and_then(move |(subj, store)| {
                            let liked = match subj {
                                Some(val) => match &val.main()[as2!(liked)] as &[Pointer] {
                                    [Pointer::Id(id)] => id.to_owned(),
                                    _ => return Either::A(future::ok((context, store, root))),
                                },
                                _ => return Either::A(future::ok((context, store, root))),
                            };

                            Either::B(
                                stream::iter_ok(
                                    elem.main()[as2!(object)]
                                        .iter()
                                        .filter_map(|f| match f {
                                            Pointer::Id(id) => Some(id.to_owned()),
                                            _ => None,
                                        })
                                        .collect::<Vec<_>>(),
                                )
                                .fold((store, liked), |(store, liked), item| {
                                    store
                                        .insert_collection(liked.to_owned(), item)
                                        .map(|store| (store, liked))
                                })
                                .map(|(store, _)| (context, store, root)),
                            )
                        }))
                    }

                    _ => Either::B(future::ok((context, store, root))),
                })
                .map_err(box_store_error),
        )
    }
}

#[cfg(test)]
mod test {
    use super::ClientLikeHandler;
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use futures::Async;
    use jsonld::nodemap::Entity;
    use kroeg_tap::{Context, MessageHandler};

    fn setup() -> (Context, TestStore) {
        TestStore::new(vec![
            object_under_test!(remote "/like" => {
                types => [as2!(Like)];
                as2!(object) => ["/object"];
            }),
            object_under_test!(local "/subject" => {
                types => [as2!(Person)];
                as2!(liked) => ["/liked"];
            }),
        ])
    }

    #[test]
    fn handles_like() {
        let (context, store) = setup();
        match ClientLikeHandler
            .handle(context, store, "/inbox".to_owned(), "/like".to_owned())
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => {
                assert!(
                    store.has_read_all(&["/like", "/subject"]),
                    "Handler did not read the Like nor the subject"
                );
                assert!(
                    store.contains("/liked", "/object"),
                    "Handler did not register the Like"
                );
            }
            Err((e, _)) => panic!("handler returned error: {}", e),
            _ => unreachable!(),
        }
    }
}
