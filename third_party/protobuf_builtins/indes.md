# Protobuf Builtin Types

This crate contains standard/builtin protobuf messages. The protos in `./proto/google/protobuf` are copied from the main protobuf repository and are accessible in other proto files via an `import "google/protobuf/x.proto";` statement.

Additionally we added `./proto/google/rpc/status.proto` into the builtins to bootstrap the RPC package.