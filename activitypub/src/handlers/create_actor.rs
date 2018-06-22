use jsonld::nodemap::{Entity, Pointer, Value};
use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

use std::error::Error;
use std::fmt;

use futures::prelude::*;

#[derive(Debug)]
pub enum CreateActorError<T>
where
    T: EntityStore,
{
    MissingRequired(String),
    ExistingPredicate(String),
    EntityStoreError(T::Error),
}

impl<T> fmt::Display for CreateActorError<T>
where
    T: EntityStore,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CreateActorError::MissingRequired(ref val) => write!(
                f,
                "The {} predicate is missing or occurs more than once",
                val
            ),
            CreateActorError::ExistingPredicate(ref val) => {
                write!(f, "The {} predicate should not have been passed", val)
            }
            CreateActorError::EntityStoreError(ref err) => {
                write!(f, "failed to get value from the entity store: {}", err)
            }
        }
    }
}

impl<T> Error for CreateActorError<T>
where
    T: EntityStore,
{
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct CreateActorHandler;

fn _ensure<T: EntityStore + 'static>(
    entity: &Entity,
    name: &str,
) -> Result<Pointer, CreateActorError<T>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(CreateActorError::MissingRequired(name.to_owned()))
    }
}

fn _set<T: EntityStore + 'static>(
    entity: &mut Entity,
    name: &str,
    val: Pointer,
) -> Result<(), CreateActorError<T>> {
    if entity[name].len() != 0 {
        Err(CreateActorError::ExistingPredicate(name.to_owned()))
    } else {
        entity.get_mut(name).push(val);
        Ok(())
    }
}

impl<T: EntityStore + 'static> MessageHandler<T> for CreateActorHandler {
    type Error = CreateActorError<T>;
    type Future = Box<Future<Item = (Context, T), Error = CreateActorError<T>> + Send>;

    #[async(boxed_send)]
    fn handle(
        self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T), CreateActorError<T>> {
        let subject = context.user.subject.to_owned();

        let mut elem = await!(store.get(elem))
            .map_err(|e| CreateActorError::EntityStoreError(e))?
            .expect("Missing the entity being handled, shouldn't happen");

        if !elem.main().types.contains(&as2!(Create).to_owned()) {
            return Ok((context, store));
        }

        let elem = _ensure(elem.main(), as2!(object))?;
        let elem = if let Pointer::Id(id) = elem {
            id
        } else {
            return Err(CreateActorError::MissingRequired(as2!(object).to_owned()));
        };

        let mut elem = await!(store.get(elem))
            .map_err(CreateActorError::EntityStoreError)?
            .unwrap();

        if !elem.main().types.contains(&as2!(Person).to_owned()) {
            return Ok((context, store));
        }

        let preferredUsername = _ensure(elem.main(), as2!(preferredUsername))?;
        let name = _ensure(elem.main(), as2!(name))?;

        let (context, mut store, inbox) = await!(assign_id(
            context,
            store,
            Some("inbox".to_owned()),
            Some(elem.id().to_owned())
        )).map_err(CreateActorError::EntityStoreError)?;
        let inbox = StoreItem::new(
            inbox.to_owned(),
            vec![(inbox.to_owned(), Entity::new(inbox.to_owned()))]
                .into_iter()
                .collect(),
        );

        let inbox = await!(store.put(inbox.id().to_owned(), inbox))
            .map_err(CreateActorError::EntityStoreError)?;

        let (context, mut store, outbox) = await!(assign_id(
            context,
            store,
            Some("outbox".to_owned()),
            Some(elem.id().to_owned())
        )).map_err(CreateActorError::EntityStoreError)?;
        let outbox = StoreItem::new(
            outbox.to_owned(),
            vec![(outbox.to_owned(), Entity::new(outbox.to_owned()))]
                .into_iter()
                .collect(),
        );

        let outbox = await!(store.put(outbox.id().to_owned(), outbox))
            .map_err(CreateActorError::EntityStoreError)?;

        _set(
            elem.main_mut(),
            ldp!(inbox),
            Pointer::Id(inbox.id().to_owned()),
        )?;
        _set(
            elem.main_mut(),
            as2!(outbox),
            Pointer::Id(outbox.id().to_owned()),
        )?;

        await!(store.put(elem.id().to_owned(), elem)).map_err(CreateActorError::EntityStoreError)?;

        Ok((context, store))
    }
}
