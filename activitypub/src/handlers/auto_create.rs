use jsonld::nodemap::Pointer;
use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

use std::error::Error;
use std::fmt;

use futures::prelude::{await, *};

pub struct AutomaticCreateHandler;

#[derive(Debug)]
pub enum AutomaticCreateError<T>
where
    T: EntityStore,
{
    NoObject,
    ImproperActivity,
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for AutomaticCreateError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AutomaticCreateError::NoObject => write!(f, "No as:object!"),
            AutomaticCreateError::ImproperActivity => {
                write!(f, "Improper activity, did you forget the as:actor?")
            }
            AutomaticCreateError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for AutomaticCreateError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

const DEFAULT_ACTIVITIES: [&'static str; 28] = [
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

impl<T: EntityStore + 'static> MessageHandler<T> for AutomaticCreateHandler {
    type Error = AutomaticCreateError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = Self::Error> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        mut context: Context,
        mut entitystore: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), AutomaticCreateError<T>> {
        let mut elem = await!(entitystore.get(elem))
            .map_err(AutomaticCreateError::EntityStoreError)?
            .expect("Missing the entity being handled, shouldn't happen");

        match object_type(&elem) {
            ObjectType::Activity => Ok((context, entitystore, elem.id().to_owned())),
            ObjectType::ImproperActivity => Err(AutomaticCreateError::ImproperActivity),
            ObjectType::Object => {
                let (_context, _store, id) = await!(assign_id(
                    context,
                    entitystore,
                    Some("activity".to_string()),
                    Some(elem.id().to_owned())
                )).map_err(AutomaticCreateError::EntityStoreError)?;

                context = _context;
                entitystore = _store;

                let mut entity = StoreItem::parse(
                    &id,
                    json!({
                    "@id": id,
                    "@type": [as2!(Create)],
                    as2!(object): [{"@id": elem.id()}]
                }),
                ).expect("cannot fail, static input");

                entity
                    .main_mut()
                    .get_mut(as2!(actor))
                    .push(Pointer::Id(context.user.subject.to_owned()));
                entity
                    .main_mut()
                    .get_mut(as2!(to))
                    .append(&mut elem.main_mut()[as2!(to)].clone());
                entity
                    .main_mut()
                    .get_mut(as2!(cc))
                    .append(&mut elem.main_mut()[as2!(cc)].clone());
                entity
                    .main_mut()
                    .get_mut(as2!(bto))
                    .append(&mut elem.main_mut()[as2!(bto)].clone());
                entity
                    .main_mut()
                    .get_mut(as2!(bcc))
                    .append(&mut elem.main_mut()[as2!(bcc)].clone());
                entity
                    .main_mut()
                    .get_mut(as2!(audience))
                    .append(&mut elem.main_mut()[as2!(audience)].clone());

                await!(entitystore.put(id.to_owned(), entity))
                    .map_err(AutomaticCreateError::EntityStoreError)?;

                Ok((context, entitystore, id))
            }
        }
    }
}
