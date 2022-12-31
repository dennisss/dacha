/// TODO: Verify this is used correctly everywhere.
pub trait ToNetworkOrder {
    fn to_network_order(self) -> Self;
}

impl ToNetworkOrder for u16 {
    fn to_network_order(self) -> Self {
        self.to_be()
    }
}

pub trait FromNetworkOrder {
    fn from_network_order(self) -> Self;
}

impl FromNetworkOrder for u16 {
    fn from_network_order(self) -> Self {
        self.to_be()
    }
}
