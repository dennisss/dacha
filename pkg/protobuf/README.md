Protocol Buffers
================

This package does parsing of .proto file descriptors, code generation, and parses proto messages in binary wire or text format. This supports proto 2 and 3.


Package Names
-------------

As Rust does not allow for arbitrary module names in files, the Rust path to the generated message types is relative to the folder/file in which the .proto file is located.

By default .proto files are compiled to corresponding .proto.rs files in the same directory.

A directory containing .proto files should contain a `mod.rs` containing `pub mod proto_file_base_name;` statements to include the generated code.

Inside of .proto files, all types are references by proto buffer package names and are not effected by the fact that they are being compiled for Rust.


Package structure:
- Need to be able to compile the descriptors 


TODOs
-----

- Validate that Proto2 enums can't be added in Proto3 messages.

- Validate that repeated fields aren't used in oneof.