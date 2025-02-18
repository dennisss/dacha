# Database Table Abstraction Layer

This crate defines the 'table' abstraction for storing and querying relational data by building on top of an existing transactional key-value database. Tables also support the traditional concept of primary/secondary indexes defined on some combination of the table columns.

We use protobuf messages to define the columns of the table (one column per message field) and add additional Rust defined attributes to specify the index definitions.

In the underlying key-value store, we simply store the serialized protobuf as the value with a special key defined as follows. The key format is similar to the [CockroachDB format](https://github.com/cockroachdb/cockroach/blob/master/docs/tech-notes/encoding.md) and is as follows:

- `[Table Id]` : varuint : Unique value per table
- `[Index Id]` : varuint : 0 for the primary key/index containing the full row value, other values for secondary indexes.
- `[... Key Values ...]` : List of the key values extracted from the row's full value.  
- `[Column Family ID]` : varuint : defaults to 0. Only serialized if non-zero.

Note that each part of the key is encoded will a lexicographically sortable compact representation with decodable length (so it is possible to segment the key afterwards assuming you know the type of each field).

## Usage

TODO