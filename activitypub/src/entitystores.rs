use jsonld::rdf::{jsonld_to_rdf, rdf_to_jsonld, BlankNodeGenerator};

use kroeg_cellar::{Error, QuadClient};
use kroeg_tap::{EntityStore, RecursiveEntityStore, StoreItem};

use std::cell::RefCell;
use std::collections::HashMap;

use futures::future;
use futures::prelude::*;

struct NodeGenerator {
    i: u32,
    map: HashMap<String, String>,
}

impl BlankNodeGenerator for NodeGenerator {
    fn generate_blank_node(&mut self, id: Option<&str>) -> String {
        if let Some(id) = id {
            if self.map.contains_key(id) {
                self.map[id].to_owned()
            } else {
                let form = format!("_:b{}", self.i);
                self.i += 1;
                self.map.insert(id.to_owned(), form.to_owned());
                form
            }
        } else {
            let form = format!("_:b{}", self.i);
            self.i += 1;
            form
        }
    }
}

#[derive(Debug)]
pub struct QuadEntityStore<T: EntityStore> {
    client: RefCell<QuadClient>,
    cache: RefCell<HashMap<String, Option<StoreItem>>>,
    next: T,
}

impl<T: EntityStore> QuadEntityStore<T> {
    pub fn new(client: QuadClient, next: T) -> QuadEntityStore<T> {
        QuadEntityStore {
            client: RefCell::new(client),
            cache: RefCell::new(HashMap::new()),
            next: next,
        }
    }
}

impl<T: EntityStore> RecursiveEntityStore<T> for QuadEntityStore<T> {
    fn next(&mut self) -> &mut T {
        &mut self.next
    }
}

impl<T: EntityStore> EntityStore for QuadEntityStore<T> {
    type Error = Error;
    type GetFuture = Box<Future<Item = Option<StoreItem>, Error = Self::Error> + Send>;
    type StoreFuture = Box<Future<Item = StoreItem, Error = Self::Error> + Send>;

    fn get(&self, path: String) -> Self::GetFuture {
        let cache = self.cache.borrow_mut();
        if cache.contains_key(&path) {
            Box::new(future::ok(cache[&path].clone()))
        } else {
            let mut client = self.client.borrow_mut();
            let quads = match client.read_quads(&path) {
                Ok(quads) => quads,
                Err(err) => return Box::new(future::err(err)),
            };

            if quads.len() == 0 {
                Box::new(self.next.get(path).map_err(|_| Error::NotFound))
            } else {
                let mut hash = HashMap::new();
                hash.insert("@default".to_owned(), quads);
                let jval = rdf_to_jsonld(hash, true, false);
                Box::new(future::ok(Some(StoreItem::parse(&path, jval).unwrap())))
            }
        }
    }

    fn put(&mut self, path: String, item: StoreItem) -> Self::StoreFuture {
        let mut cache = self.cache.borrow_mut();
        cache.remove(&path);

        let jld = item.to_json();

        let rdf = match jsonld_to_rdf(
            jld,
            &mut NodeGenerator {
                map: HashMap::new(),
                i: 0,
            },
        ) {
            Ok(rdf) => rdf,
            Err(err) => panic!("welp {}", err),
        };

        let quads = rdf.clone().remove("@default").unwrap();
        let mut client = self.client.borrow_mut();
        if let Err(err) = client.write_quads(&path, quads) {
            return Box::new(future::err(err));
        }

        Box::new(future::ok(
            StoreItem::parse(&path, rdf_to_jsonld(rdf, true, false)).unwrap(),
        ))
    }
}
