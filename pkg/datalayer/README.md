dataLayer
=========

`A single control and data plane for all your data needs`

This project is about collapsing the lines between pub/sub queue, (non-)relation database, object/blob storage, realtime streaming, caching, and processing systems.

While this system does not have its own client protocol, it 

This systems aims to be heavily tunable along the axes of:
- `persistence`: support a hybrid of in-memory, and on-disk storage backends
- `location`: data should be trivial to store in a subset of the cluster and operations should be smart enough to localize their behavior to increase performance
- `consistency`: eventual through strong consistency supported on reads and writes
- `ai`: because why not. can we accurately predict the usage patterns of clients and optimize settings such as caching and priority around that



All protocols we'd like to eventually support:
	Redis
	etcd
	MongoDB
	Cassandra
	Hadoop
	HBase
	Spark (as a Connector)
	Riak
	MySQL/PostgresQL
	Gremlin
	RabbitMQ
	https://getstream.io/
	Google gRPC defs: (PubSub, Datastore, GCS)