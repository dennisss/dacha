use std::collections::HashMap;
use alloc::string::{String, ToString};

use common::errors::*;
use common::hash::FastHasherBuilder;
use file::LocalPath;

use crate::io::csv::{CSVReader, CSVParser};


pub struct CSVDataReader {
    reader: CSVReader,
    field_indexes: HashMap<String, usize, FastHasherBuilder>
}

impl CSVDataReader {
    pub async fn create(path: &LocalPath) -> Result<Self> {
        let mut reader = CSVReader::new(file::LocalFile::open(path)?);

        let header = reader.read().await?.unwrap();

        let mut field_indexes = HashMap::<String, usize, FastHasherBuilder>::default();
        for i in 0..header.num_fields() {
            field_indexes.insert(header.field(i)?.to_string(), i);
        }


        Ok(Self {
            reader,
            field_indexes
        })
    }

    pub async fn read<'a>(&'a mut self) -> Result<Option<CSVDataRow<'a>>> {
        let row_parser = match self.reader.read().await? {
            Some(v) => v,
            None => return Ok(None)
        };

        Ok(Some(CSVDataRow {
            field_indexes: &self.field_indexes,
            row_parser
        }))
    }
}

pub struct CSVDataRow<'a> {
    field_indexes: &'a HashMap<String, usize, FastHasherBuilder>,
    row_parser: &'a CSVParser,
}

impl<'a> CSVDataRow<'a> {
    pub fn str_field(&self, name: &str) -> Result<&str> {
        let idx = *self.field_indexes.get(name).ok_or_else(|| err_msg("Missing field"))?;        
        let v = self.row_parser.field(idx)?;
        Ok(v)
    }

    pub fn f32_field(&self, name: &str) -> Result<f32> {
        let idx = *self.field_indexes.get(name).ok_or_else(|| err_msg("Missing field"))?;
        let v = self.row_parser.field(idx)?.parse()?;
        Ok(v)
    }

    pub fn f64_field(&self, name: &str) -> Result<f64> {
        let idx = *self.field_indexes.get(name).ok_or_else(|| err_msg("Missing field"))?;
        let v = self.row_parser.field(idx)?.parse()?;
        Ok(v)
    }

    pub fn optional_f32_field(&self, name: &str) -> Result<Option<f32>> {
        let idx = match self.field_indexes.get(name) {
            Some(v) => *v,
            None => return Ok(None)
        };

        let s = self.row_parser.field(idx)?;
        if s.is_empty() {
            return Ok(None);
        }

        let v = s.parse()?;
        Ok(Some(v))

    }
}
