# HTTP v2


TODO: Need to implement a keepalive pinger.
- Only needed if no other packets received recently.
- Server sends pings
    - Send every 2 minutes
    - Allow 20 seconds for response

## Shutdown Semantics

If either endpoint (Client or Server) receives a non-NO_ERROR GOAWAY packet, it will immediately close the underlying connection instead of sending a GOAWAY reply because section 5.4.1 of the spec states that the sender of a errorful GOAWAY MUST immediately close the TCP connection so there is no point in sending any packets to a remote endpoint that has sent us an error.

When the Client wants to gracefully shut down it will sent a GOAWAY to the Server with NO_ERROR and a Last-Stream-Id set the the id of the last stream id received from the server (effectively disabling the receival of additional push promises). Once all streams have finished, the Client will simply close the connection.

When the Client wants to non-gracefully shut down the connection, it will simply close the connection.

^ TODO: Technically the current implementation will send a GOAWAY packet and then immediately close the connection.

After the Client locally starts a shutdown, it will no longer send any more new requests to the server.

If the Client receives any GOAWAY packet from the Server, it will stop sending new requests to the Server. If the received GOAWAY error is NO_ERROR, then we'll locally trigger a graceful shutdown (sending a GOAWAY to the Server) assuming it hasn't already triggered a local shutdown.

^ TODO: Implement the above line (sending the second GOAWAY).

When the Server wants to gracefully shut down, it will send the client a GOAWAY message with NO_ERROR and a Last-Stream-Id set to MAX_STREAM_ID. After N seconds, if the connection is still open, the Server will send another GOAWAY message with NO_ERROR and a Last-Stream-Id set to the id of the last stream received. Finally the Server will close the TCP connection after flushing that GOAWAY.

A Server wants to non-gracefully (abruptly) shut down, will simply skip the N second wait, and send the second GOAWAY packet mentioned above first and then close the connection.

<!-- When a Server receives a GOAWAY packet with NO_ERROR from the Client, it will send a reply with a GOAWAY with NO_ERROR and a Last-Stream-Id set to the  -->


### Timeouts

TODO: The Server should support limiting the maximum duration of RPCs


TODO: "A GOAWAY frame might not immediately precede closing of the connection; a receiver of a GOAWAY that has no more use for the connection SHOULD still send a GOAWAY frame before terminating the connection."