use jsonld::nodemap::Pointer;
use kroeg_tap::{assign_id, box_store_error, Context, EntityStore, MessageHandler, StoreItem};

use std::error::Error;
use std::fmt;

use futures::{
    future::{self, Either},
    Future,
};

pub struct AutomaticCreateHandler;

#[derive(Debug)]
pub enum AutomaticCreateError {
    ImproperActivity,
}

impl fmt::Display for AutomaticCreateError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AutomaticCreateError::ImproperActivity => {
                write!(f, "Improper activity, did you forget the as:actor?")
            }
        }
    }
}

impl Error for AutomaticCreateError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

const DEFAULT_ACTIVITIES: &'static [&'static str] = &[
    as2!(Accept),
    as2!(Add),
    as2!(Announce),
    as2!(Arrive),
    as2!(Block),
    as2!(Create),
    as2!(Delete),
    as2!(Dislike),
    as2!(Flag),
    as2!(Follow),
    as2!(Ignore),
    as2!(Invite),
    as2!(Join),
    as2!(Leave),
    as2!(Like),
    as2!(Listen),
    as2!(Move),
    as2!(Offer),
    as2!(Question),
    as2!(Reject),
    as2!(Read),
    as2!(Remove),
    as2!(TentativeReject),
    as2!(TentativeAccept),
    as2!(Travel),
    as2!(Undo),
    as2!(Update),
    as2!(View),
];

enum ObjectType {
    Activity,
    ImproperActivity,
    Object,
}

fn object_type(entity: &StoreItem) -> ObjectType {
    if entity.main()[as2!(actor)].len() > 0 {
        ObjectType::Activity
    } else {
        for typ in entity.main().types.iter() {
            if DEFAULT_ACTIVITIES.contains(&&**typ) {
                return ObjectType::ImproperActivity;
            }
        }

        ObjectType::Object
    }
}

const TO_CLONE: &'static [&'static str] =
    &[as2!(to), as2!(cc), as2!(bto), as2!(bcc), as2!(audience)];

impl<T: EntityStore + 'static> MessageHandler<T> for AutomaticCreateHandler {
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Box<
        Future<Item = (Context, T, String), Error = (Box<Error + Send + Sync + 'static>, T)> + Send,
    > {
        let root = elem.to_owned();

        Box::new(
            store
                .get(elem, false)
                .map_err(box_store_error)
                .and_then(move |(elem, store)| {
                    let elem = match elem {
                        Some(elem) => elem,
                        None => return Either::A(future::ok((context, store, root))),
                    };

                    match object_type(&elem) {
                        ObjectType::Activity => {
                            return Either::A(future::ok((context, store, root)))
                        }
                        ObjectType::ImproperActivity => {
                            return Either::A(future::err((
                                AutomaticCreateError::ImproperActivity.into(),
                                store,
                            )))
                        }
                        ObjectType::Object => {}
                    }

                    Either::B(
                        assign_id(
                            context,
                            store,
                            Some("activity".to_owned()),
                            Some(root.to_owned()),
                            1,
                        )
                        .and_then(move |(context, store, id)| {
                            let mut activity = StoreItem::parse(
                                &id,
                                json!({
                        "@id": id,
                        "@type": [as2!(Create)],
                        as2!(object): [{"@id": elem.id()}],
                        as2!(actor): [{"@value": &context.user.subject}]
                    }),
                            )
                            .unwrap();

                            for predicate in TO_CLONE {
                                activity
                                    .main_mut()
                                    .get_mut(predicate)
                                    .extend_from_slice(&elem.main()[predicate]);
                            }

                            store
                                .put(activity.id().to_owned(), activity)
                                .map(move |(item, store)| (context, store, item.id().to_owned()))
                        })
                        .map_err(box_store_error),
                    )
                }),
        )
    }
}
