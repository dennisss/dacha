use std::collections::HashMap;
use std::{collections::HashSet, convert::TryFrom, sync::Arc};

use common::errors::*;
use google_auth::*;
use http::uri::Uri;

use googleapis_proto::google::spanner::admin::database::v1 as admin;
use googleapis_proto::google::spanner::v1 as spanner;
use protobuf::reflection::ReflectionMut;
use protobuf::{reflection::Reflection, FieldNumber};
use protobuf::{Message, MessageReflection};
use protobuf_builtins::google::protobuf::{ListValue, ValueProto};

use crate::sql::{
    ColumnDefinition, CreateIndexStatement, CreateTableStatement, DataType, DdlStatement,
    Direction, KeyPart, MaxLength, ScalarType,
};
use crate::SpannerDatabaseClient;

pub trait ProtobufTableTag {
    type Message: protobuf::StaticMessage;

    fn table_name(&self) -> &str;

    /// Lists all fields that are present in the primary key.
    fn indexed_keys(&self) -> Vec<ProtobufTableKey>;

    /*
            {
                name: "UserByEmailAddress"
                fields: ["email_address"]
                direction: ASCENDING
            }
    */
}

pub struct ProtobufTableKey {
    /// None implies this is the primary key
    pub index_name: Option<String>,
    pub fields: Vec<FieldNumber>,
}

pub struct ProtobufTable<'a, T: ProtobufTableTag + 'static> {
    client: &'a SpannerDatabaseClient,
    tag: &'static T,

    column_names: Vec<String>,
    column_to_field_number: HashMap<String, FieldNumber>,
    field_number_to_column: HashMap<FieldNumber, String>,
}

impl<'a, T: ProtobufTableTag + 'static> ProtobufTable<'a, T> {
    // Types are converted to ListValue values as described in
    // https://cloud.google.com/spanner/docs/reference/rpc/google.spanner.v1#typecode

    pub fn new(client: &'a SpannerDatabaseClient, tag: &'static T) -> Self {
        let mut dummy_value = T::Message::default();

        let mut column_names = vec![];
        let mut column_to_field_number = HashMap::new();
        let mut field_number_to_column = HashMap::new();
        for field in dummy_value.fields() {
            let column_name = common::snake_to_camel_case(&field.name);
            column_names.push(column_name.clone());
            column_to_field_number.insert(column_name.clone(), field.number);
            field_number_to_column.insert(field.number, column_name);
        }

        // TODO: Check that the indexes are well formed.

        Self {
            client,
            tag,
            column_names,
            column_to_field_number,
            field_number_to_column,
        }
    }

    pub fn data_definitions(&self) -> Result<Vec<DdlStatement>> {
        let mut out = vec![];

        let mut dummy_value = T::Message::default();

        let mut table_columns = vec![];
        for name in &self.column_names {
            let field_num = *self.column_to_field_number.get(name).unwrap();

            let typ = self.get_sql_type(dummy_value.field_by_number_mut(field_num).unwrap());

            table_columns.push(ColumnDefinition {
                column_name: name.clone(),
                typ,
                not_null: true,
            });
        }

        let mut primary_key = vec![];

        for indexed_key in self.tag.indexed_keys() {
            let mut key_parts = vec![];
            for field_num in indexed_key.fields {
                key_parts.push(KeyPart {
                    column_name: self.field_number_to_column.get(&field_num).unwrap().clone(),
                    direction: None,
                });
            }

            if let Some(index_name) = indexed_key.index_name {
                out.push(DdlStatement::CreateIndex(CreateIndexStatement {
                    unique: false,
                    index_name,
                    table_name: self.tag.table_name().to_string(),
                    key_parts,
                }));
            } else {
                primary_key = key_parts;
            }
        }

        // Add before the CreateIndex statements.
        out.insert(
            0,
            DdlStatement::CreateTable(CreateTableStatement {
                table_name: self.tag.table_name().to_string(),
                columns: table_columns,
                primary_key,
            }),
        );

        Ok(out)
    }

    pub async fn insert(&self, values: &[T::Message]) -> Result<()> {
        let mut converted_values = vec![];

        for value in values {
            let mut v = ListValue::default();
            self.encode_proto_row(value, &mut v)?;
            converted_values.push(v);
        }

        self.client
            .insert(
                self.tag.table_name(),
                &self.column_names[..],
                &converted_values,
            )
            .await?;

        Ok(())
    }

    pub async fn read(
        &self,
        query_fields: &[protobuf::FieldNumber],
        query_value: &T::Message,
    ) -> Result<Vec<T::Message>> {
        let mut req = googleapis_proto::google::spanner::v1::ReadRequest::default();

        req.set_table(self.tag.table_name());
        for col in &self.column_names {
            req.add_columns(col.clone());
        }

        let mut query_fields_set = HashSet::<FieldNumber>::default();
        for v in query_fields {
            query_fields_set.insert(*v);
        }

        // TODO: Need to validate that index keys actually exist in the message (also
        // the query keys)
        let mut found_index = None;
        for indexed_key in self.tag.indexed_keys() {
            let mut num_matches = 0;
            for field in &indexed_key.fields {
                if !query_fields_set.contains(field) {
                    break;
                }

                num_matches += 1;
            }

            if num_matches != query_fields_set.len() {
                continue;
            }

            found_index = Some(indexed_key);
        }

        let index_key = found_index.ok_or_else(|| err_msg("No index found"))?;
        if let Some(name) = index_key.index_name {
            req.set_index(name);
        }

        let mut list_value = protobuf_builtins::google::protobuf::ListValue::default();
        for field_num in index_key.fields {
            if !query_fields_set.contains(&field_num) {
                break;
            }

            self.encode_proto_column(
                query_value.field_by_number(field_num),
                list_value.new_values(),
            )?;
        }

        let range = req.key_set_mut().new_ranges();
        range.set_start_closed(list_value.clone());
        range.set_end_closed(list_value);

        let mut result = self.client.read(req).await?;

        let mut out = vec![];

        for row in result.rows() {
            let mut v = T::Message::default();

            for i in 0..row.values().len() {
                // TODO: Bounds check this.
                let column_name = result.metadata().row_type().fields()[i].name();
                let field_num = *self
                    .column_to_field_number
                    .get(column_name)
                    .ok_or_else(|| err_msg("Unknown column"))?;

                let column_value = row.values()[i].as_ref();
                if column_value.has_null_value() {
                    continue;
                }

                self.decode_proto_column(column_value, v.field_by_number_mut(field_num).unwrap())?;
            }

            out.push(v);
        }

        Ok(out)
    }

    fn encode_proto_row(&self, value: &T::Message, output: &mut ListValue) -> Result<()> {
        for field in value.fields() {
            let output_column = output.new_values();
            self.encode_proto_column(value.field_by_number(field.number), output_column)?;
        }

        Ok(())
    }

    fn encode_proto_column(
        &self,
        reflect: Option<Reflection>,
        value: &mut ValueProto,
    ) -> Result<()> {
        let reflect = match reflect {
            Some(v) => v,
            None => {
                // TODO: Instead return an error or verify that the field has no presence if
                // doing this.
                value.null_value_mut();
                return Ok(());
            }
        };

        /*
        Challenge:
        - Can't change any top level field from non-repeated to repeated.
        */

        match reflect {
            Reflection::F32(v) => value.set_number_value(*v as f64),
            Reflection::F64(v) => value.set_number_value(*v),
            Reflection::I32(v) => value.set_string_value(v.to_string()),
            Reflection::I64(v) => value.set_string_value(v.to_string()),
            Reflection::U32(v) => value.set_string_value(v.to_string()),
            Reflection::U64(v) => value.set_string_value(v.to_string()),
            Reflection::Bool(v) => value.set_bool_value(*v),
            Reflection::String(v) => value.set_string_value(v),
            Reflection::Bytes(v) => {
                value.set_string_value(base_radix::base64_encode(v));
            }
            Reflection::Repeated(_) => todo!(),
            Reflection::Message(v) => {
                // BYTES
                if v.type_url()
                    == protobuf_builtins::google::protobuf::Timestamp::default().type_url()
                {
                    // Downcast
                }

                let data = v.serialize()?;
                value.set_string_value(base_radix::base64_encode(&data));
            }
            Reflection::Enum(v) => {
                // INT32
                value.set_string_value(v.value().to_string());
            }
            Reflection::Set(_) => todo!(),
        }

        Ok(())
    }

    fn get_sql_type(&self, reflect: ReflectionMut) -> DataType {
        match reflect {
            ReflectionMut::Bool(_) => DataType {
                is_array: false,
                scalar_type: ScalarType::Bool,
            },
            ReflectionMut::F32(_) | ReflectionMut::F64(_) => DataType {
                is_array: false,
                scalar_type: ScalarType::Numeric,
            },
            ReflectionMut::I32(_)
            | ReflectionMut::I64(_)
            | ReflectionMut::U32(_)
            | ReflectionMut::U64(_)
            | ReflectionMut::Enum(_) => DataType {
                is_array: false,
                scalar_type: ScalarType::Int64,
            },
            ReflectionMut::String(_) => DataType {
                is_array: false,
                scalar_type: ScalarType::String(MaxLength::IntMax),
            },
            ReflectionMut::Bytes(_) => DataType {
                is_array: false,
                scalar_type: ScalarType::Bytes(MaxLength::IntMax),
            },

            ReflectionMut::Message(_) => {
                // TODO: Handle special message type cases.

                DataType {
                    is_array: false,
                    scalar_type: ScalarType::Bytes(MaxLength::IntMax),
                }
            }
            ReflectionMut::Repeated(v) => {
                // NOTE: repeated fields can't be nested.
                let scalar_type = self.get_sql_type(v.reflect_add()).scalar_type;
                DataType {
                    is_array: true,
                    scalar_type,
                }
            }
            ReflectionMut::Set(v) => {
                // NOTE: repeated fields can't be nested.
                let scalar_type = self.get_sql_type(v.entry_mut().value()).scalar_type;
                DataType {
                    is_array: true,
                    scalar_type,
                }
            }
        }
    }

    fn decode_proto_column(&self, value: &ValueProto, reflect: ReflectionMut) -> Result<()> {
        match reflect {
            ReflectionMut::F32(v) => {
                if !value.has_number_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.number_value() as f32;
            }
            ReflectionMut::F64(v) => {
                if !value.has_number_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.number_value();
            }
            ReflectionMut::I32(v) => {
                if !value.has_string_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.string_value().parse()?;
            }
            ReflectionMut::I64(v) => {
                if !value.has_string_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.string_value().parse()?;
            }
            ReflectionMut::U32(v) => {
                if !value.has_string_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.string_value().parse()?;
            }
            ReflectionMut::U64(v) => {
                if !value.has_string_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.string_value().parse()?;
            }
            ReflectionMut::Bool(v) => {
                if !value.has_bool_value() {
                    return Err(err_msg("Wrong format"));
                }

                *v = value.bool_value();
            }
            ReflectionMut::String(v) => {
                if !value.has_string_value() {
                    return Err(err_msg("Wrong format"));
                }

                v.push_str(value.string_value());
            }
            ReflectionMut::Bytes(v) => {
                if !value.has_string_value() {
                    return Err(err_msg("Wrong format"));
                }

                let data = base_radix::base64_decode(value.string_value())?;
                v.extend_from_slice(&data);
            }
            ReflectionMut::Repeated(_) => todo!(),
            ReflectionMut::Message(_) => todo!(),
            ReflectionMut::Enum(_) => todo!(),
            ReflectionMut::Set(_) => todo!(),
        }

        Ok(())
    }
}
