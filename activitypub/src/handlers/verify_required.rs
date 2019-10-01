use jsonld::nodemap::Pointer;
use std::error::Error;
use std::fmt;
use url::Url;

use kroeg_tap::{as2, Context, MessageHandler};

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

impl Error for RequiredEventsError {}

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

#[async_trait::async_trait]
impl MessageHandler for VerifyRequiredEventsHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        _inbox: &mut String,
        id: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let val = context
            .entity_store
            .get(id.to_owned(), false)
            .await?
            .ok_or(RequiredEventsError::FailedToRetrieve)?;

        let actor = match &val.main()[as2!(actor)] as &[Pointer] {
            [] => return Err(RequiredEventsError::MissingActor.into()),

            // A. The actor is the same user as the authorized account. This is if doing e.g. follower delivery
            // In fan-out of reply posts, Mastodon might send out posts to other sharedInboxes. This is fine, but:
            // B. Either the actor is authorized properly (if the user is on the same server) in which case we can trust the data.
            // C. Or the actor is authorized on another origin (when on another server) which means we don't trust the data in the POST.
            //    (The incoming data has already been limited to be the same origin as the authorized account, yay!)
            // If the actor is authorized on the same origin, but different actor, this is probably spoofing. While there is no way to
            //    re-retrieve the data from the remote origin, there's no reason for this situation to ever occur. Let's just error out.
            [Pointer::Id(actor)]
                if actor == &context.user.subject
                    || (!same_origin(actor, &context.user.subject)
                        && same_origin(actor, val.id())) =>
            {
                actor.clone()
            }
            _ => return Err(RequiredEventsError::NotAllowedtoAct.into()),
        };

        let local_post = self.0;

        let applies_to_types = if local_post {
            APPLIES_TO_TYPES_LOCAL
        } else {
            APPLIES_TO_TYPES_REMOTE
        };

        if !val
            .main()
            .types
            .iter()
            .any(|f| applies_to_types.contains(&(&f as &str)))
        {
            return Ok(());
        }

        let elem = if let [Pointer::Id(id)] = &val.main()[as2!(object)] as &[Pointer] {
            context
                .entity_store
                .get(id.to_owned(), false)
                .await?
                .ok_or(RequiredEventsError::MissingObject)?
        } else {
            return Err(RequiredEventsError::MissingObject.into());
        };

        if elem.main().types.iter().any(|f| f == as2!(Tombstone)) {
            return Ok(());
        }

        let pointer = Pointer::Id(actor.clone());
        if !elem.main()[as2!(attributedTo)].contains(&pointer)
            || (local_post && elem.main()[as2!(attributedTo)].len() != 1 && elem.id() != actor)
            || (!local_post && elem.main()[as2!(attributedTo)].is_empty())
        {
            return Err(RequiredEventsError::ActorAttributedToDoNotMatch.into());
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{RequiredEventsError, VerifyRequiredEventsHandler};
    use crate::test::TestStore;
    use crate::{handle_object_pair, object_under_test};
    use async_std::task::block_on;
    use kroeg_tap::{as2, MessageHandler};

    fn setup() -> (TestStore, ()) {
        (
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
                    as2!(actor) => ["/subject"];
                    // no actor, no object
                }),
                object_under_test!(remote "https://example.com/origin" => {
                    types => [as2!(Create)];
                    as2!(actor) => ["https://example.com/actor"];
                    as2!(object) => ["https://example.com/object"];
                }),
                object_under_test!(remote "https://example.com/object" => {
                    types => [as2!(Note)];
                    as2!(attributedTo) => ["https://example.com/actor"];
                }),
                object_under_test!(remote "https://example.com/origin/b" => {
                    types => [as2!(Create)];
                    as2!(actor) => ["https://example.com/actor"];
                    as2!(object) => ["https://example.com/object/b"];
                }),
                object_under_test!(remote "https://example.com/origin/c" => {
                    types => [as2!(Announce)];
                    as2!(actor) => ["https://example.com/actor"];
                    as2!(object) => ["https://contoso.com/actor"];
                }),
                object_under_test!(remote "https://example.com/object/b" => {
                    types => [as2!(Note)];
                    as2!(attributedTo) => ["https://contoso.com/actor"];
                }),
            ]),
            (),
        )
    }

    #[test]
    fn remote_object_two_attributed() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(VerifyRequiredEventsHandler(false).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/a".to_owned(),
        )) {
            Ok(()) => {
                assert!(
                    store.has_read_all(&["/a", "/a/object"]),
                    "Handler did not read all the expected objects"
                );
            }

            Err(e) => panic!("handler refused object: {}", e),
        }
    }

    #[test]
    fn local_object_invalid_attributed() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(VerifyRequiredEventsHandler(true).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/a".to_owned(),
        )) {
            Ok(()) => panic!("handler accepted object"),
            Err(e) => match e.downcast() {
                Ok(val) => match *val {
                    RequiredEventsError::ActorAttributedToDoNotMatch => { /* ok! */ }
                    e => panic!("handler refused object for wrong reason: {}", e),
                },

                Err(e) => panic!("handler refused object: {}", e),
            },
        }
    }

    #[test]
    fn valid_object() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(VerifyRequiredEventsHandler(true).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/b".to_owned(),
        )) {
            Ok(()) => { /* ok! */ }
            Err(e) => panic!("handler refused object: {}", e),
        }
    }

    #[test]
    fn announce() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(VerifyRequiredEventsHandler(true).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "/c".to_owned(),
        )) {
            Ok(()) => { /* ok! */ }
            Err(e) => panic!("handler refused object: {}", e),
        }
    }

    #[test]
    fn same_origin() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(VerifyRequiredEventsHandler(true).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "https://example.com/origin".to_owned(),
        )) {
            Ok(()) => { /* ok! */ }
            Err(e) => panic!("handler refused object: {}", e),
        }
    }

    #[test]
    fn different_origin() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);

        match block_on(VerifyRequiredEventsHandler(true).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "https://example.com/origin/b".to_owned(),
        )) {
            Ok(()) => panic!("handler accepted object"),
            Err(e) => match e.downcast() {
                Ok(val) => match *val {
                    RequiredEventsError::ActorAttributedToDoNotMatch => { /* ok! */ }
                    e => panic!("handler refused object: {}", e),
                },

                Err(e) => panic!("handler refused object: {}", e),
            },
        }
    }

    #[test]
    fn different_origin_announce() {
        let (mut store, mut queue) = setup();
        let mut context = store.context(&mut queue);
        match block_on(VerifyRequiredEventsHandler(true).handle(
            &mut context,
            &mut "/inbox".to_owned(),
            &mut "https://example.com/origin/c".to_owned(),
        )) {
            Ok(()) => { /* ok! */ }
            Err(e) => panic!("handler refused object: {}", e),
        }
    }
}
