

Out of order serialization strategies to implement:

- Support we want to serialize a length field before the data it is referencing (and we don't know the length of the data without serializing it):
    - Serialize the data first into a separate rope and concatenate with the length field rope at the end.
    - If the length field is a primitive, pre-allocate zero'ed bytes in the buffer for storing it
        - Then serialize the data field and go edit the earlier bytes in the buffer to fix the length field to the right value.



TODO: Represent the serialization operations as a DAG of operations so that its easiest to reason about their ordering.



Parsing DSL

- By defining fields in a textproto, this should allow us to automatically create a struct which implements Default, parse and serialize

- Can we implement the protobuf format using this?