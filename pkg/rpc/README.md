# RPC

Provides a standard interface for calling methods on a server. This is currently built to be
compatible with the gRPC over HTTP2 protocol.

Error codes returned are compatible with: https://grpc.github.io/grpc/core/md_doc_statuscodes.html


## TODOs

// TODO: Technically in GRPC, the END_STREAM HTTP2 flag must always occur on a DATA frame:
// "In scenarios where the Request stream needs to be closed but no data remains to be sent implementations MUST send an empty DATA frame with this flag set."


TODO: Force usage of HTTP2