/*
2-bits of states: choked? interested?


Handshake:
    character ninteen (decimal) followed by the string [19] 'BitTorrent protocol'.

    8 reserved bytes (all zeros)

    20 byte info_hash
        If both sides don't sent the sent thing, close the connection.

    20 byte peer id
        verify matches value in tracker list or close connection


Messages
    Length prefixed (length of 0 is a keep alive sent every 2 minuts)

    First byte is the type:
        0 - choke (no payload)
        1 - unchoke (no payload)
        2 - interested (no payload)
        3 - not interested (no payload)
        4 - have
            Single number payload (index of a piece that the downloader has and has validated)
        5 - bitfield
            Always the first message sent by a downloader. List of all pieces it has sent.
        6 - request
            messages contain an index, begin, and length. The last two are byte offsets. Length is generally a power of two unless it gets truncated by the end of the file. All current implementations use 2^14 (16 kiB), and close connections which request an amount greater than that.
        7 - piece
            contain an index, begin, and piece. Note that they are correlated with request messages implicitly. It's possible for an unexpected piece to arrive if choke and unchoke messages are sent in quick succession and/or transfer is going very slowly.
        8 - cancel




Ints are 4-byte Big endian


UPnP or NAT-PMP for getting a port?
PEX for more peers
DHT to find more peers

TODO: Encryption?

Encrpytion spec:
    https://wiki.vuze.com/w/Message_Stream_Encryption


General client interface:
- Each connection is 1 torrent file
- Need to be able to read any individual piece
- Need to be able to receive the data for a peice
    - May want some type of flow control if we are receiving things too quickly

*/
