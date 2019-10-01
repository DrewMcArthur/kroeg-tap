use crate::entity::StoreItem;
use crate::user::Context;

use jsonld::nodemap::{Pointer, Value};
use serde_json::Value as JValue;
use std::error::Error;

#[async_trait::async_trait]
pub trait Authorizer: Send + Sync + 'static {
    async fn can_show(
        &self,
        context: &mut Context<'_, '_>,
        entity: &StoreItem,
    ) -> Result<bool, Box<dyn Error + Send + Sync + 'static>>;
}

#[async_trait::async_trait]
impl Authorizer for () {
    async fn can_show(
        &self,
        _: &mut Context<'_, '_>,
        _: &StoreItem,
    ) -> Result<bool, Box<dyn Error + Send + Sync + 'static>> {
        Ok(true)
    }
}

pub struct DefaultAuthorizer;

#[async_trait::async_trait]
impl Authorizer for DefaultAuthorizer {
    async fn can_show(
        &self,
        context: &mut Context<'_, '_>,
        entity: &StoreItem,
    ) -> Result<bool, Box<dyn Error + Send + Sync + 'static>> {
        let mut audience = Vec::new();
        let mut has_non_actor = false;
        for item in &[
            as2!(to),
            as2!(cc),
            as2!(bcc),
            as2!(bto),
            as2!(actor),
            as2!(object),
            as2!(attributedTo),
        ] {
            for it in &entity.main()[item] {
                if item != &as2!(actor) && item != &as2!(attributedTo) && item != &as2!(object) {
                    has_non_actor = true;
                }

                if let Pointer::Id(it) = it {
                    audience.push(it.to_owned());
                }
            }
        }

        if !has_non_actor {
            Ok(true)
        } else if audience.contains(&as2!(Public).to_string())
            || audience.contains(&context.user.subject)
        {
            Ok(true)
        } else {
            for id in audience {
                let data = context
                    .entity_store
                    .find_collection(id, context.user.subject.clone())
                    .await?;

                if !data.items.is_empty() {
                    return Ok(true);
                }
            }

            Ok(false)
        }
    }
}

pub struct LocalOnlyAuthorizer<R>(R);

impl<R: Authorizer> LocalOnlyAuthorizer<R> {
    pub fn new(authorizer: R) -> LocalOnlyAuthorizer<R> {
        LocalOnlyAuthorizer(authorizer)
    }
}

#[async_trait::async_trait]
impl<R: Authorizer> Authorizer for LocalOnlyAuthorizer<R> {
    async fn can_show(
        &self,
        context: &mut Context<'_, '_>,
        entity: &StoreItem,
    ) -> Result<bool, Box<dyn Error + Send + Sync + 'static>> {
        let is_local = match entity
            .sub(kroeg!(meta))
            .and_then(|f| f[kroeg!(instance)].get(0))
        {
            Some(Pointer::Value(Value {
                value: JValue::Number(num),
                ..
            })) => num
                .as_u64()
                .map(|f| f == context.instance_id as u64)
                .unwrap_or(false),
            _ => false,
        };

        if !is_local {
            Ok(false)
        } else {
            self.0.can_show(context, entity).await
        }
    }
}
