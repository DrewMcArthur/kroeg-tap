use jsonld::nodemap::{Entity, Pointer};

use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

use std::error::Error;
use std::fmt;

use futures::prelude::*;

#[derive(Debug)]
pub enum ClientUndoError<T>
where
    T: EntityStore,
{
    DifferingActor,
    MissingRequired(String),
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for ClientUndoError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ClientUndoError::DifferingActor => write!(f, "as:actor on Undo and object differ!"),

            ClientUndoError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            ClientUndoError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for ClientUndoError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

fn _ensure<T: EntityStore + 'static>(
    entity: &Entity,
    name: &str,
) -> Result<Pointer, ClientUndoError<T>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(ClientUndoError::MissingRequired(name.to_owned()))
    }
}

fn equals_any_order(a: &Vec<Pointer>, b: &Vec<Pointer>) -> bool {
    if a.len() != b.len() {
        return false;
    }

    for item in a {
        if !b.contains(item) {
            return false;
        }
    }

    true
}

pub struct ClientUndoHandler;

impl<T: EntityStore + 'static> MessageHandler<T> for ClientUndoHandler {
    type Error = ClientUndoError<T>;
    type Future = Box<Future<Item = (Context, T, String), Error = ClientUndoError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        mut context: Context,
        mut store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), ClientUndoError<T>> {
        let subject = context.user.subject.to_owned();
        let root = elem.to_owned();

        let mut relem = await!(store.get(elem))
            .map_err(|e| ClientUndoError::EntityStoreError(e))?
            .expect("Missing the entity being handled, shouldn't happen");

        if !relem.main().types.contains(&as2!(Undo).to_owned()) {
            return Ok((context, store, root));
        }

        let elem = _ensure(relem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(ClientUndoError::MissingRequired(as2!(object).to_owned()));
        };

        let mut elem = await!(store.get(elem.to_owned()))
            .map_err(ClientUndoError::EntityStoreError)?
            .unwrap();

        if !equals_any_order(&relem.main()[as2!(actor)], &elem.main()[as2!(actor)]) {
            return Err(ClientUndoError::DifferingActor);
        }

        let mut subj = await!(store.get(subject))
            .map_err(ClientUndoError::EntityStoreError)?
            .unwrap();

        if elem.main().types.contains(&as2!(Like).to_owned()) {
            if let Some(Pointer::Id(liked)) = subj.main()[as2!(liked)].iter().next().cloned() {
                await!(store.remove_collection(liked.to_owned(), elem.id().to_owned()))
                    .map_err(ClientUndoError::EntityStoreError)?
            }
        }

        if elem.main().types.contains(&as2!(Follow).to_owned()) {
            if let Some(Pointer::Id(following)) =
                subj.main()[as2!(following)].iter().next().cloned()
            {
                await!(store.remove_collection(following.to_owned(), elem.id().to_owned()))
                    .map_err(ClientUndoError::EntityStoreError)?
            }
        }

        Ok((context, store, root))
    }
}
