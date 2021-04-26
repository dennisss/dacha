

/*
Ordered list of name-value paris (can have duplicates)

NOTE: Separate state for request and resposne


A change in the maximum size of the dynamic table is signaled via a
   dynamic table size update (see Section 6.3).  This dynamic table size
   update MUST occur at the beginning of the first header block
   following the change to the dynamic table size.  In HTTP/2, this
   follows a settings acknowledgment (see Section 6.5.3 of [HTTP2]).

Dynamic Table is FIFO (oldest has higher index)
- May contain duplicate entries.
*/

/*
    What's the minium full buffer size?
    - N 
*/

// Max size is obtained from SETTINGS_HEADER_TABLE_SIZE

use common::{errors::*};
use parsing::parse_next;
use parsing::binary::be_u8;







struct Decoder {
    // dynamic_table
}

