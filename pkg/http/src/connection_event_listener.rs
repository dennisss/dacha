/// Object which is notified when certain connection events occur.
///
/// NOTE: The connection instance will likely be internally locked when the
/// events are called so these listeners should not depend on reading the
/// connection.
#[async_trait]
pub trait ConnectionEventListener: Send + Sync + 'static {
    /// Called when the connection begins to shutdown and stops accepting new
    /// requests.
    ///
    /// This is guranteed to be called before in-flight requests are notified
    /// that they are about to fail due to the connection failure.
    ///
    /// This is guranteed to be called only exactly once in a connection's
    /// lifetime.
    async fn handle_connection_shutdown(&self, details: ConnectionShutdownDetails);

    /// Called whenever a request has been completely processed (after the last
    /// byte of the response is done being read).
    ///
    /// The listener may not get called if the connection is closing. In that
    /// case, run() will return shortly after the last sent request's
    /// completion.
    async fn handle_request_completed(&self);

    /// For HTTP2 connections, notifies the user that we can a request/response
    /// for a ping from the other side.
    async fn handle_ping_response(&self, opaque_data: u64, is_ack: bool);
}

#[derive(Clone, Copy)]
pub struct ConnectionShutdownDetails {
    /// The connection was gracefully shut down rather than failing.
    ///
    /// The definition of 'graceful' is:
    /// - No client requests.
    /// - For HTTP1 connections:
    ///   - Received 'Connection: Close'
    ///   - Or the underlying TCP connection got a FIN packet (TODO: Implement
    ///     this).
    /// - For HTTP2 connections, we received a GOAWAY with NO_ERROR.
    pub graceful: bool,

    /// If true, this shutdown was requested by a local caller. Else, this was
    /// due to some remote or connection event.
    pub local: bool,

    pub http1_rejected_persistence: bool,
}
