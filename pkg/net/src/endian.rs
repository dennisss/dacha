/// TODO: Verify this is used correctly everywhere.
pub trait ToNetworkOrder {
    fn to_network_order(self) -> Self;
}

impl ToNetworkOrder for u16 {
    fn to_network_order(self) -> Self {
        self.to_be()
    }
}
