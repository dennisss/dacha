pub const GRPC_PROTO_TYPE: &'static str = "application/grpc+proto";

/// Name of the trailer header used for returning the gRPC status code.
pub const GRPC_STATUS: &'static str = "grpc-status";

pub const GRPC_STATUS_MESSAGE: &'static str = "grpc-message";

/// Name of the binary trailers header containing a serialized google.rpc.Status
/// protobuf with additional error details. The code and message in this proto
/// should always match the above dedicated status/message headers.
pub const GRPC_STATUS_DETAILS: &'static str = "grpc-status-details-bin";
