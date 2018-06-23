use jsonld::nodemap::{Entity, Pointer, Value};

use serde_json::Value as JValue;

use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use openssl::rsa::Rsa;
use openssl::pkey::{Private, Public};
use openssl::error::ErrorStack;

use futures::prelude::*;

#[derive(Debug)]
pub enum CreateActorError<T>
where
    T: EntityStore,
{
    MissingRequired(String),
    ExistingPredicate(String),
    OpenSSLError(ErrorStack),
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
            CreateActorError::OpenSSLError(ref err) => {
                write!(f, "failed to do RSA key magic: {}", err)
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

    fn create_key_obj<T: EntityStore + 'static>(owner: &str) -> Result<(Rsa<Private>, StoreItem), CreateActorError<T>> {
        let id = format!("{}#key", owner);

        let key = Rsa::generate(2048).map_err(CreateActorError::OpenSSLError)?;
        let private_pem = String::from_utf8(key.private_key_to_pem().map_err(CreateActorError::OpenSSLError)?).unwrap();
        let public_pem = String::from_utf8(key.public_key_to_pem().map_err(CreateActorError::OpenSSLError)?).unwrap();

        let mut keyobj = Entity::new(id.to_owned());
        keyobj.types.push(sec!(Key).to_owned());
        keyobj[sec!(owner)].push(Pointer::Id(owner.to_owned()));
        keyobj[sec!(publicKeyPem)].push(Pointer::Value(Value {
            value: JValue::String(public_pem),
            type_id: None,
            language: None
        }));

        let mut map = HashMap::new();
        map.insert(id.to_owned(), keyobj);

        let mut storeitem = StoreItem::new(id, map);
        storeitem.meta()[sec!(privateKeyPem)].push(Pointer::Value(Value {
            value: JValue::String(private_pem),
            type_id: None,
            language: None
        }));

        Ok((key, storeitem))
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


        let (key, keyobj) = create_key_obj(elem.id())?;

        _set(elem.main_mut(), sec!(publicKey), Pointer::Id(keyobj.id().to_owned()))?;

        await!(store.put(keyobj.id().to_owned(), keyobj)).map_err(CreateActorError::EntityStoreError)?;
        await!(store.put(elem.id().to_owned(), elem)).map_err(CreateActorError::EntityStoreError)?;
        Ok((context, store))
    }
}
