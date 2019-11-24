
Channels
--------

Like queues except optimized for distribution of a small number of messages to a non-majority of nodes (then relayed to subscribed clients connected to those nodes)

For every channel we host a replicated list of which nodes what to hear about messages in the channel
- Adding a subscription requires appending to this list for the first client on a single node


Emitting a message
- Each node will hold a local list of all 
- Look up list stored locally of nodes
- Then just send out a message to each of those nodes


Multi-cast
- Many times, we will have identical messages that need to go to many channels
	- For this, we will allow local unioning of the node lists for all channels and then send a message to every single one of these 