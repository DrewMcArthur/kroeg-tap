use jsonld::nodemap::Pointer;

use kroeg_tap::{box_store_error, Context, EntityStore, MessageHandler};

use std::error::Error;
use std::fmt;

use futures::{
    future::{self, Either},
    stream, Future, Stream,
};

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

impl Error for ServerCreateError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct ServerCreateHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ServerCreateHandler {
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Box<
        Future<Item = (Context, T, String), Error = (Box<Error + Send + Sync + 'static>, T)>
            + Send
            + 'static,
    > {
        let root = elem.to_owned();
        Box::new(
            store
                .get(elem, false)
                .map_err(box_store_error)
                .and_then(|(val, store)| match val {
                    Some(val) if val.main().types.contains(&as2!(Create).to_owned()) => {
                        let id = match &val.main()[as2!(object)] as &[Pointer] {
                            [Pointer::Id(id)] => id.to_owned(),
                            _ => {
                                return Either::A(future::err((
                                    ServerCreateError::MissingObject.into(),
                                    store,
                                )))
                            }
                        };

                        Either::B(store.get(id, false).map_err(box_store_error).and_then(
                            |(val, store)| {
                                match val {
                                    Some(val) => future::ok((
                                        val.main()[as2!(inReplyTo)]
                                            .iter()
                                            .filter_map(|f| {
                                                if let Pointer::Id(id) = f {
                                                    Some(id.to_owned())
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect(),
                                        store,
                                        val.id().to_owned(),
                                    )),
                                    None => future::err((
                                        ServerCreateError::FailedToRetrieve.into(),
                                        store,
                                    )),
                                }
                            },
                        ))
                    }
                    Some(val) => Either::A(future::ok((vec![], store, val.id().to_owned()))),
                    None => Either::A(future::err((
                        ServerCreateError::FailedToRetrieve.into(),
                        store,
                    ))),
                })
                .and_then(|(items, store, elem)| {
                    stream::iter_ok(items)
                        .fold((context, store, elem), |(context, store, elem), item| {
                            store
                                .get(item, true)
                                .and_then(move |(item, store)| match item {
                                    Some(item) if item.is_owned(&context) => {
                                        match &item.main()[as2!(replies)] as &[Pointer] {
                                            [Pointer::Id(id)] => Either::A(
                                                store
                                                    .insert_collection(
                                                        id.to_owned(),
                                                        elem.to_owned(),
                                                    )
                                                    .map(move |store| (context, store, elem)),
                                            ),
                                            _ => Either::B(future::ok((context, store, elem))),
                                        }
                                    }

                                    _ => Either::B(future::ok((context, store, elem))),
                                })
                        })
                        .map(|(context, store, _)| (context, store, root))
                        .map_err(box_store_error)
                }),
        )
    }
}

#[cfg(test)]
mod test {
    use super::{ServerCreateError, ServerCreateHandler};
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
            object_under_test!(local "/local" => {
                types => [as2!(Note)];
                as2!(replies) => ["/replies"];
            }),
            object_under_test!(local "/local/create" => {
                types => [as2!(Create)];
                as2!(object) => ["/local/object"];
            }),
            object_under_test!(local "/local/object" => {
                types => [as2!(Note)];
                as2!(inReplyTo) => ["/local"];
            }),
        ])
    }

    #[test]
    fn ignores_non_create() {
        let (context, store) = setup();
        match ServerCreateHandler
            .handle(context, store, "/inbox".to_owned(), "/like".to_owned())
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => {
                assert!(
                    store.has_read("/like"),
                    "Handler did not read the root object"
                );
                assert!(
                    !store.has_read("/a/object"),
                    "Handler attempted to process the Like"
                );
            }
            Err((e, _)) => panic!("handler returned error: {}", e),
            _ => unreachable!(),
        }
    }

    #[test]
    fn handles_local_object() {
        let (context, store) = setup();
        match ServerCreateHandler
            .handle(
                context,
                store,
                "/inbox".to_owned(),
                "/local/create".to_owned(),
            )
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => {
                assert!(
                    store.has_read_all(&["/local/create", "/local/object", "/local"]),
                    "Handler did not read all objects"
                );
                assert!(
                    store.contains("/replies", "/local/object"),
                    "Reply has not been recorded properly"
                );
            }
            Err((e, _)) => panic!("handler returned error: {}", e),
            _ => unreachable!(),
        }
    }
}
