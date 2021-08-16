/*

struct OUT {
    value: u32
    // Internally is also aware of its own address.
}

impl OUT {
    fn load() -> Self;
    fn store(&self);

    fn reset();

    // Read accessors for
}



General idea:
- build time rule
- Outputs: One file per SVD:


peripherals.p0().out().set_pin0_high();

Every register value to be represented by a packed struct with just one field (the value of that )
- Need two views of each register:
    - The readable and writable portions.



Lot's of different SVD files located here:
- https://github.com/posborne/cmsis-svd/tree/master/data

For each peripheral:
    For each register:
        pub mod PERIPHERAL_NAME {
            pub const REGISTER_NAME: *mut u32 = ADDRESS as *mut u32;
        }


TODO: headerStructName on the peripheral
*/

// denniss.me
// dennis.page
