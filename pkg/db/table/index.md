# Database Table Abstraction Layer

This crate defines the 'table' abstraction for storing and querying relational data by building on top of an existing transactional key-value database. Tables also support the traditional concept of primary/secondary indexes defined on some combination of the table columns.

We use protobuf messages to define the columns of the table (one column per message field) and add additional Rust defined attributes to specify the index definitions.

In the underlying key-value store, we simply store the serialized protobuf as the value with a special key defined as follows. The key format is similar to the [CockroachDB format](https://github.com/cockroachdb/cockroach/blob/master/docs/tech-notes/encoding.md) and is as follows:

- `[Table Id]` : varuint : Unique value per table
- `[Index Id]` : varuint : 0 for the primary key/index containing the full row value, other values for secondary indexes.
- `[... Key Values ...]` : List of the key values extracted from the row's full value.  
- `[Column Family ID]` : varuint : defaults to 0. Only serialized if non-zero.

Note that each part of the key is encoded will a lexicographically sortable compact representation with decodable length (so it is possible to segment the key afterwards assuming you know the type of each field).

A value of each key-value row is generated as follows:

- For the primary key row, this is just the serialized binary proto excluding any fields specified in the key.
- For index fields, the value is the primary key serialized as a binary proto excluding any fields specified in the key.

In both cases, the protos are serialized deterministically (ascending field order in wire format) to enable future optimizations to take advance of the sorted field ordering to do faster comparisons to a sorted query.

## Usage

TODO


## Data Types

Any arbitrary protobuf can be stored as a row in a table. But when defining keys and queries, there is only support for using primitive values (integers, bools, string, bytes, enums). Important semantics are listed below:

- Signs are preserved when doing query operations (`-3 i64 is < 1 u64`) and there is no 'bit-casting' so be mindful of the types passed to the library.
- Field presence is ignored and there is no concept of 'null'. So fields in an index are always indexed (if necessary, will a default value of the field).

## Query Execution

A basic query execution engine is implemented the vast majority of 'efficient' queries which we expect can be 'efficiently' executed using either a direct primary key scan or a indirect scan using a single secondary key scan followed by point lookups in the primary key. More complex queries than this are likely to fall back to (near) full table scans. Our definition of 'efficiency' here is minimizing the number of rows scanned in the database per row returned in a query. The best case is 
1:1.

The query execution process goes as follows:

- **Parsing**: Given a user's query in an 'SQL WHERE' style text format, we will first parse it and convert it to disjunctive normal form (DNF aka 'OR of AND statements'). e.g. `(a > 1 AND b = 2) OR (a < 1 AND b = 4)`. This form is convenient as it will allow us to treat each AND clause (e.g. `a > 1 AND b = 2`) as a candidate key range in a table index.

- Each AND clause (e.g. `(a < 1 AND b = 4)`) is processed separately as follows:

- **Index Evaluation**:
    - For each primary/secondary index, we attempt to generate the longest prefix key query on that index using the AND clause's components (this will serve a candidate key range over which we will retrieve values).
    - The cost of each index is calculated.
        - The cost is an estimation of the `num_scanned_rows:num_output_rows` ratio/fanout. A perfect cost score is `1`.
        - Each of the N fields in the AND clause is given a score as follows:
            - If the field if part of the generated index key prefix:
                - `2` if the key is represented as a range in the prefix and there are some fields in the query that weren't matched by the prefix (this is meant to represent the fact that the range may overselect values of the field after additional filtering is applied).
                - `1` otherwise 
            - `4` otherwise
        - The scores of all of the fields are multiplied together to get the index cost.
            - We also `+ 1` to the final cost if the index is not the primary key.
        - The index with smallest cost is chosen to serve the query.
            - Ties will use the index with the smaller index.
    - If the same index is also the best choice for any following AND clauses, they are merged into the same query and will be retrieved together.
        - TODO: Implement this part.

- **Row Retrieval**:
    - If we chose to use the primary index, we scan over the previously generated longest key prefix and select all seen rows.
    - Else, we chose a secondary index, we will:
        - Scan the secondary index key prefix for primary key values.
        - Do point lookups on the primary index to select all of those keys.
            - TODO: Apply prefiltering to the primary key values if there are any unfiltered primary key values in the query (we shouldn't change the cost calculation based on this to still encourage more sharing of indexes between sub queries).

- **Post Filtering**: Since not all queried fields may be indexed, we apply a simple check to see if the original query actually matches the row to filter out rows.
    - TODO: Remove all fields matched by the keys from this filter to speed up the computation.

- **Deduplication**: An in-memory hash set is used to deduplicate any values that we've already seen based on key.
    - TODO: Think about whether or not we can put this earlier in the query process to avoid filtering.




