use entity::StoreItem;
use entitystore::EntityStore;
use jsonld::nodemap::Pointer;
use user::Context;

use futures::future;
use futures::prelude::*;

pub trait Authorizer<T: EntityStore>: Send + Sync + 'static {
    type Future: Future<Item = (T, bool), Error = T::Error> + 'static + Send;

    fn can_show(&self, store: T, entity: &StoreItem) -> Self::Future;
}

impl<T: EntityStore> Authorizer<T> for () {
    type Future = future::FutureResult<(T, bool), T::Error>;

    fn can_show(&self, store: T, entity: &StoreItem) -> Self::Future {
        future::ok((store, true))
    }
}

pub struct DefaultAuthorizer(String);

impl DefaultAuthorizer {
    pub fn new(context: &Context) -> DefaultAuthorizer {
        DefaultAuthorizer(context.user.subject.to_owned())
    }
}

#[async(boxed_send)]
fn recursive_verify<T: EntityStore>(
    subject: String,
    store: T,
    ids: Vec<String>,
) -> Result<(T, bool), T::Error> {
    if ids.contains(&as2!(Public).to_string()) {
        Ok((store, true))
    } else if ids.contains(&subject) {
        Ok((store, true))
    } else {
        for id in ids.into_iter() {
            let data = await!(store.find_collection(id, subject.to_owned()))?;
            if data.items.len() != 0 {
                return Ok((store, true));
            }
        }

        Ok((store, false))
    }
}

impl<T: EntityStore> Authorizer<T> for DefaultAuthorizer {
    type Future = Box<Future<Item = (T, bool), Error = T::Error> + 'static + Send>;

    fn can_show(&self, store: T, entity: &StoreItem) -> Self::Future {
        let mut audience = Vec::new();
        let mut has_non_actor = false;
        for item in &[
            as2!(to),
            as2!(cc),
            as2!(bcc),
            as2!(bto),
            as2!(actor),
            as2!(attributedTo),
        ] {
            for it in &entity.main()[item] {
                if item != &as2!(actor) && item != &as2!(attributedTo) {
                    has_non_actor = true;
                }

                if let Pointer::Id(it) = it {
                    audience.push(it.to_owned());
                }
            }
        }

        if !has_non_actor {
            Box::new(future::ok((store, true)))
        } else {
            recursive_verify(self.0.to_owned(), store, audience)
        }
    }
}
