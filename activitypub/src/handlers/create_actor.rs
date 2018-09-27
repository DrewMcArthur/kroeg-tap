use jsonld::nodemap::{Entity, Pointer, Value};

use serde_json::Value as JValue;

use kroeg_tap::{assign_id, Context, EntityStore, MessageHandler, StoreItem};

use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use openssl::error::ErrorStack;
use openssl::pkey::Private;
use openssl::rsa::Rsa;

use futures::prelude::{await, *};

#[derive(Debug)]
pub enum CreateActorError {
    MissingRequired(String),
    ExistingPredicate(String),
    OpenSSLError(ErrorStack),
}

impl fmt::Display for CreateActorError {
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
        }
    }
}

impl Error for CreateActorError {
    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub struct CreateActorHandler;

fn _ensure(entity: &Entity, name: &str) -> Result<Pointer, Box<Error + Send + Sync + 'static>> {
    if entity[name].len() == 1 {
        Ok(entity[name][0].to_owned())
    } else {
        Err(Box::new(CreateActorError::MissingRequired(name.to_owned())))
    }
}

fn _set(
    entity: &mut Entity,
    name: &str,
    val: Pointer,
) -> Result<(), Box<Error + Send + Sync + 'static>> {
    if entity[name].len() != 0 {
        Err(Box::new(CreateActorError::ExistingPredicate(
            name.to_owned(),
        )))
    } else {
        entity.get_mut(name).push(val);
        Ok(())
    }
}

fn create_key_obj(
    owner: &str,
) -> Result<(Rsa<Private>, StoreItem), Box<Error + Send + Sync + 'static>> {
    let id = format!("{}#key", owner);

    let key = Rsa::generate(2048).map_err(|e| Box::new(CreateActorError::OpenSSLError(e)))?;
    let private_pem = String::from_utf8(
        key.private_key_to_pem()
            .map_err(|e| Box::new(CreateActorError::OpenSSLError(e)))?,
    ).unwrap();
    let public_pem = String::from_utf8(
        key.public_key_to_pem()
            .map_err(|e| Box::new(CreateActorError::OpenSSLError(e)))?,
    ).unwrap();

    let mut keyobj = Entity::new(id.to_owned());
    keyobj.types.push(sec!(Key).to_owned());
    keyobj[sec!(owner)].push(Pointer::Id(owner.to_owned()));
    keyobj[sec!(publicKeyPem)].push(Pointer::Value(Value {
        value: JValue::String(public_pem),
        type_id: None,
        language: None,
    }));

    let mut map = HashMap::new();
    map.insert(id.to_owned(), keyobj);

    let mut storeitem = StoreItem::new(id, map);
    storeitem.meta()[sec!(privateKeyPem)].push(Pointer::Value(Value {
        value: JValue::String(private_pem),
        type_id: None,
        language: None,
    }));

    Ok((key, storeitem))
}

fn create_collection(id: &str, owned: &str, boxtype: &str) -> StoreItem {
    let mut item = StoreItem::parse(
        id,
        json!({
                "@id": id,
                "@type": [as2!(OrderedCollection)],
                as2!(partOf): [{"@id": owned}]
            }),
    ).unwrap();

    item.meta()[kroeg!(box)].push(Pointer::Id(boxtype.to_owned()));

    item
}

#[async]
fn assign_and_store<T: EntityStore + 'static>(
    context: Context,
    mut store: T,
    parent: String,
    object: String,
) -> Result<(Context, T, String), T::Error> {
    let (context, mut store, collection) = await!(assign_id(
        context,
        store,
        Some(object),
        Some(parent.to_owned())
    ))?;

    let collection = await!(
        store.put(
            collection.to_owned(),
            StoreItem::parse(
                &collection,
                json!({
                "@id": &collection,
                "@type": [as2!(OrderedCollection)],
                as2!(partOf): [{"@id": &parent}]
            })
            ).unwrap()
        )
    )?;

    Ok((context, store, collection.id().to_owned()))
}

impl<T: EntityStore + 'static> MessageHandler<T> for CreateActorHandler {
    #[async(boxed_send)]
    fn handle(
        &self,
        context: Context,
        store: T,
        _inbox: String,
        elem: String,
    ) -> Result<(Context, T, String), Box<Error + Send + Sync + 'static>> {
        let root = elem.to_owned();

        let mut elem = await!(store.get(elem, false))
            .map_err(Box::new)?
            .expect("Missing the entity being handled, shouldn't happen");

        let mut elem = if elem.main()[as2!(preferredUsername)].len() > 0
            || elem.main().types.contains(&as2!(Person).to_owned())
        {
            elem
        } else if !elem.main().types.contains(&as2!(Create).to_owned()) {
            let elem = _ensure(elem.main(), as2!(object))?;
            let elem = if let Pointer::Id(id) = elem {
                id
            } else {
                return Err(Box::new(CreateActorError::MissingRequired(
                    as2!(object).to_owned(),
                )));
            };

            await!(store.get(elem, false)).map_err(Box::new)?.unwrap()
        } else {
            return Ok((context, store, root));
        };

        if !elem.main().types.contains(&as2!(Person).to_owned()) {
            return Ok((context, store, root));
        }

        _ensure(elem.main(), as2!(preferredUsername))?;
        _ensure(elem.main(), as2!(name))?;

        let (context, mut store, inbox) = await!(assign_id(
            context,
            store,
            Some("inbox".to_owned()),
            Some(elem.id().to_owned())
        )).map_err(Box::new)?;

        let inbox = await!(store.put(
            inbox.to_owned(),
            create_collection(&inbox, elem.id(), ldp!(inbox))
        )).map_err(Box::new)?;

        let (context, mut store, outbox) = await!(assign_id(
            context,
            store,
            Some("outbox".to_owned()),
            Some(elem.id().to_owned())
        )).map_err(Box::new)?;
        let outbox = await!(store.put(
            outbox.to_owned(),
            create_collection(&outbox, elem.id(), as2!(outbox))
        )).map_err(Box::new)?;

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

        let (context, mut store, following) = await!(assign_and_store(
            context,
            store,
            elem.id().to_owned(),
            String::from("following")
        )).map_err(Box::new)?;
        let (context, mut store, followers) = await!(assign_and_store(
            context,
            store,
            elem.id().to_owned(),
            String::from("followers")
        )).map_err(Box::new)?;

        _set(elem.main_mut(), as2!(following), Pointer::Id(following))?;

        _set(elem.main_mut(), as2!(followers), Pointer::Id(followers))?;

        let (_, keyobj) = create_key_obj(elem.id())?;

        _set(
            elem.main_mut(),
            sec!(publicKey),
            Pointer::Id(keyobj.id().to_owned()),
        )?;

        await!(store.put(keyobj.id().to_owned(), keyobj)).map_err(Box::new)?;
        await!(store.put(elem.id().to_owned(), elem)).map_err(Box::new)?;
        Ok((context, store, root))
    }
}
