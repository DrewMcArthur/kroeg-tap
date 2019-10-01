use jsonld::nodemap::{Entity, Pointer, Value};
use openssl::rsa::Rsa;
use serde_json::json;
use serde_json::Value as JValue;
use std::collections::HashMap;
use std::error::Error;

use kroeg_tap::{as2, assign_id, kroeg, ldp, sec, Context, MessageHandler, StoreItem};

pub struct CreateActorHandler;

fn create_key_obj(owner: &str) -> Result<StoreItem, Box<dyn Error + Send + Sync + 'static>> {
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
        &json!({
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

async fn add_all_collections(
    context: &mut Context<'_, '_>,
    item: &mut StoreItem,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    for (typ, key, boxtype) in COLLECTIONS {
        if !item.main()[key].is_empty() {
            return Err(format!("predicate {} already has value while creating user", key).into());
        }

        let collection_id = assign_id(
            context,
            Some((*typ).to_owned()),
            Some(item.id().to_owned()),
            1,
        )
        .await?;
        let mut collection = build_collection(&collection_id, item.id(), *boxtype, context);
        item.main_mut()
            .get_mut(key)
            .push(Pointer::Id(collection_id.clone()));

        context
            .entity_store
            .put(collection_id, &mut collection)
            .await?;
    }

    if item.main()[sec!(publicKey)].len() != 0 {
        return Err("predicate publicKey already has value while creating user".into());
    }

    let mut key = create_key_obj(item.id())?;

    item.main_mut()[sec!(publicKey)].push(Pointer::Id(key.id().to_owned()));

    context
        .entity_store
        .put(key.id().to_owned(), &mut key)
        .await?;

    context.entity_store.put(item.id().to_owned(), item).await
}

#[async_trait::async_trait]
impl MessageHandler for CreateActorHandler {
    async fn handle(
        &self,
        context: &mut Context<'_, '_>,
        _inbox: &mut String,
        elem: &mut String,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let elem = match context.entity_store.get(elem.to_owned(), false).await? {
            Some(elem) => elem,
            None => return Ok(()),
        };

        let mut person = if elem.main().types.iter().any(|f| f == as2!(Person)) {
            elem
        } else if elem.main().types.iter().any(|f| f == as2!(Create)) {
            if let [Pointer::Id(obj)] = &elem.main()[as2!(object)] as &[Pointer] {
                match context.entity_store.get(obj.to_owned(), false).await? {
                    Some(item) if item.main().types.iter().any(|f| f == as2!(Person)) => item,
                    _ => return Ok(()),
                }
            } else {
                return Ok(());
            }
        } else {
            return Ok(());
        };

        add_all_collections(context, &mut person).await
    }
}
