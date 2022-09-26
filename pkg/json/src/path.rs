use common::errors::*;
use parsing::*;

use crate::parser::{parse_number, parse_string};
use crate::Value;

pub trait ValuePath {
    fn path(&self, path: &str) -> Result<Option<&Value>>;
}

impl ValuePath for Value {
    fn path(&self, path: &str) -> Result<Option<&Value>> {
        let path = Path::parse(path)?;
        Ok(path.select(self))
    }
}

pub struct Path {
    segments: Vec<PathSegment>,
}

enum PathSegment {
    Key(String),
    Index(usize),
}

impl Path {
    pub fn parse(mut input: &str) -> Result<Self> {
        parse_next!(input, tag("$"));

        let mut segments = vec![];

        while !input.is_empty() {
            segments.push(parse_next!(input, segment));
        }

        Ok(Self { segments })
    }

    pub fn select<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        let mut current = root;
        for segment in &self.segments {
            let next_value = match segment {
                PathSegment::Key(key) => current.get_field(key.as_str()),
                PathSegment::Index(idx) => current.get_element(*idx),
            };

            match next_value {
                Some(v) => current = v,
                None => return None,
            }
        }

        Some(current)
    }
}

parser!(segment<&str, PathSegment> => alt!(
    seq!(c => {
        c.next(tag("."))?;
        let key = c.next(ident)?;
        Ok(PathSegment::Key(key))
    }),
    seq!(c => {
        c.next(tag("["))?;
        let seg = c.next(index)?;
        c.next(tag("]"))?;
        Ok(seg)
    })
));

// TODO: Deduplicate with protobuf.
parser!(ident<&str, String> => {
    map(slice(seq!(c => {
        c.next(like(|c: char| { c.is_alphabetic() || c == '_' }))?;
        c.next(take_while(|c: char| {
            c.is_alphanumeric() || c == '_'
        }))?;
        Ok(())
    })), |s: &str| s.to_owned())
});

parser!(index<&str, PathSegment> => {
    alt!(
        // TODO: Validate only positive integers provided.
        map(parse_number, |v| PathSegment::Index(v as usize)),
        map(|v| parse_string(true)(v), |s| PathSegment::Key(s))
    )
});
