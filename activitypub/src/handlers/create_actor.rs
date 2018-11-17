use jsonld::nodemap::{Entity, Pointer, Value};

use serde_json::Value as JValue;

use kroeg_tap::{assign_id, box_store_error, Context, EntityStore, MessageHandler, StoreItem};

use openssl::rsa::Rsa;
use std::collections::HashMap;
use std::error::Error;

use futures::{
    future::{self, Either},
    stream, Future, Stream,
};

pub struct CreateActorHandler;

fn create_key_obj(owner: &str) -> Result<StoreItem, Box<Error + Send + Sync + 'static>> {
    let id = format!("{}#public-key", owner);
    let key = Rsa::generate(2048)?;

    let private_pem = String::from_utf8(key.private_key_to_pem()?)?;
    let public_pem = String::from_utf8(key.public_key_to_pem()?)?;

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

    Ok(storeitem)
}

fn build_collection(id: &str, owned: &str, boxtype: Option<&str>, context: &Context) -> StoreItem {
    let mut item = StoreItem::parse(
        id,
        json!({
            "@id": id,
            "@type": [as2!(OrderedCollection)],
            as2!(partOf): [{"@id": owned}]
        }),
    )
    .unwrap();

    if let Some(boxtype) = boxtype {
        item.meta()[kroeg!(box)].push(Pointer::Id(boxtype.to_owned()));
    }

    item.meta()[kroeg!(instance)].push(Pointer::Value(Value {
        value: context.instance_id.into(),
        type_id: Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
        language: None,
    }));

    item
}

// inbox, outbox, following, followers, liked
const COLLECTIONS: &'static [(&'static str, &'static str, Option<&'static str>)] = &[
    ("inbox", ldp!(inbox), Some(ldp!(inbox))),
    ("outbox", as2!(outbox), Some(as2!(outbox))),
    ("following", as2!(following), None),
    ("followers", as2!(followers), None),
    ("liked", as2!(liked), None),
];

fn add_all_collections<T: EntityStore>(
    item: StoreItem,
    store: T,
    context: Context,
) -> impl Future<Item = (Context, T, StoreItem), Error = (Box<Error + Send + Sync + 'static>, T)> {
    stream::iter_ok(COLLECTIONS.iter())
        .fold(
            (item, store, context),
            |(mut item, store, context), (typ, key, boxtype)| {
                if item.main()[key].len() != 0 {
                    return Either::A(future::err((
                        format!("predicate {} already exists", key).into(),
                        store,
                    )));
                }

                Either::B(
                    assign_id(
                        context,
                        store,
                        Some(typ.to_string()),
                        Some(item.id().to_owned()),
                        1,
                    )
                    .and_then(move |(context, store, collection_id)| {
                        let collection =
                            build_collection(&collection_id, item.id(), *boxtype, &context);

                        item.main_mut()
                            .get_mut(key)
                            .push(Pointer::Id(collection_id.to_owned()));

                        store
                            .put(collection_id, collection)
                            .map(move |(_, store)| (item, store, context))
                    })
                    .map_err(box_store_error),
                )
            },
        )
        .and_then(|(mut item, store, context)| {
            if item.main()[sec!(publicKey)].len() != 0 {
                return Either::A(future::err((
                    "predicate publicKey already exists".into(),
                    store,
                )));
            }

            let key = match create_key_obj(item.id()) {
                Ok(key) => key,
                Err(e) => return Either::A(future::err((e.into(), store))),
            };

            item.main_mut()[as2!(publicKey)].push(Pointer::Id(key.id().to_owned()));

            Either::B(
                store
                    .put(key.id().to_owned(), key)
                    .and_then(move |(_, store)| store.put(item.id().to_owned(), item))
                    .map(|(item, store)| (context, store, item))
                    .map_err(box_store_error),
            )
        })
}

impl<T: EntityStore + 'static> MessageHandler<T> for CreateActorHandler {
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
                .and_then(move |(elem, store)| {
                    let person_future = match elem {
                        Some(elem) if elem.main().types.contains(&as2!(Person).to_owned()) => {
                            Either::A(future::ok((Some(elem), store)))
                        }
                        Some(elem) if elem.main().types.contains(&as2!(Create).to_owned()) => match &elem.main()[as2!(object)] as &[Pointer] {
                            [Pointer::Id(obj)] => {
                                Either::B(store.get(obj.to_owned(), false).map(|(item, store)| {
                                    (
                                        item.and_then(|f| {
                                            if f.main().types.contains(&as2!(Person).to_owned()) {
                                                Some(f)
                                            } else {
                                                None
                                            }
                                        }),
                                        store,
                                    )
                                }))
                            }
                            _ => return Either::A(future::ok((context, store, root))),
                        },

                        _ => return Either::A(future::ok((context, store, root))),
                    }
                    .map_err(box_store_error);

                    Either::B(person_future.and_then(move |(item, store)| {
                        match item {
                            Some(item) => Either::A(
                                add_all_collections(item, store, context)
                                    .map(move |(context, store, _)| (context, store, root)),
                            ),
                            None => Either::B(future::ok((context, store, root))),
                        }
                    }))
                }),
        )
    }
}
