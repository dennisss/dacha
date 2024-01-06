pub mod discovery_client;
pub mod discovery_server;
pub mod multicast;
pub mod route_channel;
pub mod route_resolver;
pub mod route_store;

/*
Components that we need:
- RouteStore
    - To store mappings from (group id, server ids) to addresses
- DiscoveryClient
    - Uses a seed list to periodicall probe for
- DiscoveryServer


- RouteChannelFactory
    - Contains an Arc<RouteStore>
- RouteChannel
    - Contains a ServerId, GroupId, Arc<RouterStore>
    - Also a cached rpc::Http2Channel
    - If we

Challenges:
- A leader may contact us before we know it's route
- This doesn't solve for how to get a route to other components (user RPC)
    -

*/
