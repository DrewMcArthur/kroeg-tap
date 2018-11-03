use std::num::ParseIntError;
use std::str::FromStr;

/// An ID value in a query.
#[derive(Debug)]
pub enum QueryId {
    /// A static ID value.
    Value(String),

    /// A placeholder ID value. When queried, all the placeholders in a query with the same number have the same value.
    Placeholder(u32),

    /// Matches any of these values.
    Any(Vec<String>),

    /// Don't match on anything.
    Ignore,
}

impl FromStr for QueryId {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(if s == "_" {
            QueryId::Ignore
        } else if s.starts_with("<") {
            QueryId::Value(s[1..s.len() - 1].to_owned())
        } else if s.contains(":") && !s.contains("://") {
            let split: Vec<_> = s.split(':').collect();
            if split.len() != 2 {
                QueryId::Value(s.to_owned())
            } else {
                match split[0] {
                    "as" => QueryId::Value(format!(
                        "https://www.w3.org/ns/activitystreams#{}",
                        split[1]
                    )),
                    "kroeg" => {
                        QueryId::Value(format!("https://puckipedia.com/kroeg/ns#{}", split[1]))
                    }
                    "ldp" => QueryId::Value(format!("http://www.w3.org/ns/ldp#{}", split[1])),
                    "ostatus" => QueryId::Value(format!("http://ostatus.org#{}", split[1])),
                    "rdf" => QueryId::Value(format!(
                        "http://www.w3.org/1999/02/22-rdf-syntax-ns#{}",
                        split[1]
                    )),
                    "schema" => QueryId::Value(format!("http://schema.org#{}", split[1])), // XXX fix in Mastodon. maybe add to supplement?
                    "toot" => QueryId::Value(format!("http://joinmastodon.org/ns#{}", split[1])),
                    "xsd" => {
                        QueryId::Value(format!("http://www.w3.org/2001/XMLSchema#{}", split[1]))
                    }
                    _ => QueryId::Value(s.to_owned()),
                }
            }
        } else if s.starts_with("?") {
            QueryId::Placeholder(s[1..].parse()?)
        } else {
            QueryId::Value(s.to_owned())
        })
    }
}

#[derive(Debug)]
pub enum QueryObject {
    Id(QueryId),
    Object { value: String, type_id: QueryId },
    LanguageString { value: String, language: String },
}

impl FromStr for QueryObject {
    type Err = <QueryId as FromStr>::Err;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.starts_with("\"") {
            let (one, two) = s[1..].split_at(s[1..].find('"').unwrap());
            if two.starts_with("\"^^") {
                Ok(QueryObject::Object {
                    value: one.to_owned(),
                    type_id: two[3..].parse()?,
                })
            } else if two.starts_with("\"@") {
                Ok(QueryObject::LanguageString {
                    value: one.to_owned(),
                    language: two[2..].to_owned(),
                })
            } else {
                Ok(QueryObject::Object {
                    value: one.to_owned(),
                    type_id: two[1..].parse()?,
                })
            }
        } else {
            Ok(QueryObject::Id(s.parse()?))
        }
    }
}

#[derive(Debug)]
pub struct QuadQuery(pub QueryId, pub QueryId, pub QueryObject);

impl FromStr for QuadQuery {
    type Err = ParseIntError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (first, s) = s.split_at(s.find(' ').unwrap());
        let (second, third) = s[1..].split_at(s[1..].find(' ').unwrap());
        let third = &third[1..];

        Ok(QuadQuery(first.parse()?, second.parse()?, third.parse()?))
    }
}
