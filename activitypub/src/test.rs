use futures::future::{ok, FutureResult};
use kroeg_tap::{CollectionPointer, Context, EntityStore, QuadQuery, StoreItem, User};
use std::collections::{HashMap, HashSet};
use std::error::Error;

#[derive(Debug)]
pub struct TestStore {
    data: HashMap<String, StoreItem>,
    items: HashMap<String, Vec<String>>,
    reads: HashSet<String>,
}

impl EntityStore for TestStore {
    type Error = !;
    type GetFuture = FutureResult<(Option<StoreItem>, Self), (Self::Error, Self)>;
    type StoreFuture = FutureResult<(StoreItem, Self), (Self::Error, Self)>;
    type ReadCollectionFuture = FutureResult<(CollectionPointer, Self), (Self::Error, Self)>;
    type WriteCollectionFuture = FutureResult<Self, (Self::Error, Self)>;
    type QueryFuture = FutureResult<(Vec<Vec<String>>, Self), (Self::Error, Self)>;

    fn get(mut self, path: String, local: bool) -> Self::GetFuture {
        println!("store: get {} (local: {})", path, local);
        self.reads.insert(path.to_owned());

        ok((self.data.get(&path).cloned(), self))
    }

    fn put(mut self, path: String, item: StoreItem) -> Self::StoreFuture {
        println!("store: put {}", path);
        self.data.insert(path, item.clone());

        ok((item, self))
    }

    fn query(self, _query: Vec<QuadQuery>) -> Self::QueryFuture {
        ok((vec![], self))
    }

    fn read_collection(
        self,
        path: String,
        _count: Option<u32>,
        _cursor: Option<String>,
    ) -> Self::ReadCollectionFuture {
        println!("store: read collection {}", path);
        ok((
            CollectionPointer {
                items: self.items.get(&path).cloned().unwrap_or_else(|| vec![]),
                after: None,
                before: None,
                count: None,
            },
            self,
        ))
    }

    fn find_collection(self, path: String, item: String) -> Self::ReadCollectionFuture {
        unimplemented!();
    }

    fn read_collection_inverse(self, item: String) -> Self::ReadCollectionFuture {
        unimplemented!();
    }

    fn insert_collection(mut self, path: String, item: String) -> Self::WriteCollectionFuture {
        println!("store: insert collection {}, item {}", path, item);
        if let None = self.items.get(&path) {
            self.items.insert(path.to_owned(), vec![]);
        }

        let list = self.items.get_mut(&path).unwrap();
        if !list.contains(&item) {
            list.push(item);
        }

        ok(self)
    }

    fn remove_collection(mut self, path: String, item: String) -> Self::WriteCollectionFuture {
        println!("store: remove collection {}, item {}", path, item);
        if let None = self.items.get(&path) {
            return ok(self);
        }

        let list = self.items.get_mut(&path).unwrap();
        if let Some(index) = list.iter().position(|f| f == &item) {
            list.remove(index);
        }

        ok(self)
    }
}

impl TestStore {
    pub fn new(data: Vec<StoreItem>) -> (Context, TestStore) {
        let store = TestStore {
            data: data.into_iter().map(|f| (f.id().to_owned(), f)).collect(),
            items: HashMap::new(),
            reads: HashSet::new(),
        };

        let context = Context {
            user: User {
                claims: HashMap::new(),
                issuer: None,
                subject: "/subject".to_owned(),
                audience: vec![],
                token_identifier: "test".to_owned(),
            },

            server_base: "".to_owned(),
            instance_id: 1,
        };

        (context, store)
    }

    pub fn contains(&self, val: &str, item: &str) -> bool {
        self.items
            .get(&String::from(val))
            .map(|f| f.iter().any(|v| v == item))
            .unwrap_or(false)
    }

    pub fn has_read(&self, val: &str) -> bool {
        self.reads.contains(&String::from(val))
    }

    pub fn has_read_all(&self, val: &[&str]) -> bool {
        let mut result = true;

        for item in val {
            result = result && self.has_read(*item)
        }

        result
    }
}

#[macro_export]
macro_rules! handle_object_pair {
    ($entity:expr, ) => {};

    ($entity:expr, types => $values:expr ; $($tail:tt)* ) => {{
        $entity.types.extend($values.iter().map(|f| f.to_string()));

        handle_object_pair!($entity, $($tail)*);
    }};

    ($entity:expr, $key:expr => $values:expr ; $($tail:tt)* ) => {{
        use jsonld::nodemap::Pointer;
        $entity.get_mut($key).extend($values.iter().map(|f| Pointer::Id(f.to_string())));

        handle_object_pair!($entity, $($tail)*);
    }};
}

#[macro_export]
macro_rules! object_under_test {
    (remote $name:expr => { $($pairs:tt)* }) => {{
        use jsonld::nodemap::Entity;
        use kroeg_tap::StoreItem;
        use std::collections::HashMap;

        let mut item = Entity::new($name.to_string());
        handle_object_pair! { item, $($pairs)* };

        let mut map = HashMap::new();
        map.insert($name.to_string(), item);

        StoreItem::new($name.to_string(), map)
    }};

    (local $name:expr => $pairs:tt ) => {{
        use jsonld::nodemap::{Pointer, Value};

        let mut item = object_under_test!(remote $name => $pairs);

        item.meta().get_mut("https://puckipedia.com/kroeg/ns#instance").push(
            Pointer::Value(Value {
                value: 1.into(),
                type_id: Some("http://www.w3.org/2001/XMLSchema#integer".to_owned()),
                language: None
            })
        );

        item
    }};
}
