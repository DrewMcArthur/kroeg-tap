use futures::{
    future::{self, Either},
    Future,
};
use jsonld::nodemap::Pointer;
use kroeg_tap::{box_store_error, Context, EntityStore, MessageHandler};
use std::error::Error;
use std::fmt;
use url::Url;

#[derive(Debug)]
pub enum RequiredEventsError {
    FailedToRetrieve,
    MissingObject,
    MissingActor,

    MayNotPublish,
    NotAllowedtoAct,
    ActorAttributedToDoNotMatch,
}

impl fmt::Display for RequiredEventsError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RequiredEventsError::FailedToRetrieve => {
                write!(f, "Failed to retrieve object. Timeout?")
            }
            RequiredEventsError::MissingObject => {
                write!(f, "as:object predicate is missing or invalid")
            }
            RequiredEventsError::MissingActor => {
                write!(f, "as:actor predicate is missing or invalid")
            }
            RequiredEventsError::MayNotPublish => {
                write!(f, "current authorization token may not publish")
            }
            RequiredEventsError::NotAllowedtoAct => write!(
                f,
                "as:actor and subject in authorization token do not correspond"
            ),
            RequiredEventsError::ActorAttributedToDoNotMatch => {
                write!(f, "as:actor and as:attributedTo in the object do not match")
            }
        }
    }
}

impl Error for RequiredEventsError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct VerifyRequiredEventsHandler(pub bool);

fn same_origin(a: &str, b: &str) -> bool {
    match (Url::parse(a), Url::parse(b)) {
        (Ok(a), Ok(b)) => a.origin() == b.origin(),

        _ => false,
    }
}

// It makes no sense to filter anything that isn't Create/Update, so list them here
const APPLIES_TO_TYPES_REMOTE: &[&'static str] = &[as2!(Create), as2!(Update)];
const APPLIES_TO_TYPES_LOCAL: &[&'static str] = &[as2!(Create), as2!(Update), as2!(Delete)];

impl<T: EntityStore + 'static> MessageHandler<T> for VerifyRequiredEventsHandler {
    fn handle(
        &self,
        context: Context,
        entitystore: T,
        _inbox: String,
        elem: String,
    ) -> Box<
        Future<Item = (Context, T, String), Error = (Box<Error + Send + Sync + 'static>, T)> + Send,
    > {
        let root = elem.to_owned();
        let subject = context.user.subject.to_owned();
        let local_post = self.0;

        Box::new(
            entitystore
                .get(elem, false)
                .map_err(box_store_error)
                .and_then(move |(val, store)| match val {
                    Some(val)
                        if val.main().types.iter().any(|f| {
                            if local_post {
                                APPLIES_TO_TYPES_LOCAL
                            } else {
                                APPLIES_TO_TYPES_REMOTE
                            }
                            .contains(&(&*f as &str))
                        }) =>
                    {
                        match &val.main()[as2!(actor)] as &[Pointer] {
                            [] => future::err((RequiredEventsError::MissingActor.into(), store)),
                            // actor and the object of the actor being on the same origin are safe,
                            // both will be compromised at the same time
                            [Pointer::Id(actor)]
                                if actor == &subject || same_origin(actor, val.id()) =>
                            {
                                future::ok((Some((actor.to_owned(), val)), store))
                            }
                            _ => future::err((RequiredEventsError::NotAllowedtoAct.into(), store)),
                        }
                    }
                    Some(_) => future::ok((None, store)),
                    _ => future::err((RequiredEventsError::FailedToRetrieve.into(), store)),
                })
                .and_then(move |(val, store)| match val {
                    Some((subject, val)) => {
                        match &(val.main()[as2!(object)].clone()) as &[Pointer] {
                            [Pointer::Id(id)] => Either::A(
                                store
                                    .get(id.to_owned(), false)
                                    .map(move |(elem, store)| (Some((subject, val, elem)), store))
                                    .map_err(box_store_error),
                            ),

                            _ => Either::B(future::err((
                                RequiredEventsError::MissingObject.into(),
                                store,
                            ))),
                        }
                    }

                    None => Either::B(future::ok((None, store))),
                })
                .and_then(move |(val, store)| match val {
                    Some((actor, val, Some(elem))) => {
                        let pointer = Pointer::Id(actor.to_owned());
                        if !elem.main()[as2!(attributedTo)].contains(&pointer)
                            || (local_post
                                && elem.main()[as2!(attributedTo)].len() != 1
                                && elem.id() != actor)
                        {
                            future::err((
                                RequiredEventsError::ActorAttributedToDoNotMatch.into(),
                                store,
                            ))
                        } else {
                            future::ok((context, store, val.id().to_owned()))
                        }
                    }

                    Some(_) => future::err((RequiredEventsError::MissingObject.into(), store)),
                    None => future::ok((context, store, root)),
                }),
        )
    }
}

#[cfg(test)]
mod test {
    use super::{RequiredEventsError, VerifyRequiredEventsHandler};
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use futures::Async;
    use jsonld::nodemap::Entity;
    use kroeg_tap::{Context, MessageHandler};

    fn setup() -> (Context, TestStore) {
        TestStore::new(vec![
            object_under_test!(local "/subject" => {
                types => [as2!(Person)];
            }),
            object_under_test!(remote "/a" => {
                types => [as2!(Create)];
                as2!(object) => ["/a/object"];
                as2!(actor) => ["/subject"];
            }),
            object_under_test!(remote "/a/object" => {
                types => [as2!(Note)];
                as2!(attributedTo) => ["/subject", "/a/actor"];
            }),
            object_under_test!(remote "/b" => {
                types => [as2!(Create)];
                as2!(actor) => ["/subject"];
                as2!(object) => ["/b/object"];
            }),
            object_under_test!(remote "/b/object" => {
                types => [as2!(Note)];
                as2!(attributedTo) => ["/subject"];
            }),
            object_under_test!(remote "/c" => {
                types => [as2!(Announce)];
                // no actor, no object
            }),
        ])
    }

    #[test]
    fn remote_object_two_attributed() {
        let (context, store) = setup();
        match VerifyRequiredEventsHandler(false)
            .handle(context, store, "/inbox".to_owned(), "/a".to_owned())
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => {
                assert!(
                    store.has_read_all(&["/a", "/a/object"]),
                    "Handler did not read all the expected objects"
                );
            }
            Err((e, _)) => panic!("handler refused object: {}", e),
            _ => unreachable!(),
        }
    }

    #[test]
    fn local_object_invalid_attributed() {
        let (context, store) = setup();
        match VerifyRequiredEventsHandler(true)
            .handle(context, store, "/inbox".to_owned(), "/a".to_owned())
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => panic!("handler accepted object"),
            Err((e, _)) => match e.downcast() {
                Ok(val) => match *val {
                    RequiredEventsError::ActorAttributedToDoNotMatch => { /* ok! */ }
                    e => panic!("handler refused object for wrong reason: {}", e),
                },

                Err(e) => panic!("handler refused object: {}", e),
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn valid_object() {
        let (context, store) = setup();
        match VerifyRequiredEventsHandler(true)
            .handle(context, store, "/inbox".to_owned(), "/b".to_owned())
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => { /* ok! */ }
            Err((e, _)) => panic!("handler refused object: {}", e),
            _ => unreachable!(),
        }
    }

    #[test]
    fn announce() {
        let (context, store) = setup();
        match VerifyRequiredEventsHandler(true)
            .handle(context, store, "/inbox".to_owned(), "/c".to_owned())
            .poll()
        {
            Ok(Async::Ready((context, store, elem))) => { /* ok! */ }
            Err((e, _)) => panic!("handler refused object: {}", e),
            _ => unreachable!(),
        }
    }
}
