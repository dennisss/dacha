use std::sync::Arc;

use base_util::atomic::AtomicArc;

use crate::tls::options::{ClientOptions, ServerOptions};

macro_rules! define_container {
    ($name:ident, $t:ty) => {
        #[derive(Clone)]
        pub struct $name {
            inner: Arc<AtomicArc<$t>>,
        }

        impl $name {
            pub fn get(&self) -> Arc<$t> {
                self.inner.load().unwrap()
            }

            pub fn set(&self, value: Arc<$t>) {
                self.inner.store(Some(value));
            }
        }

        impl From<$t> for $name {
            fn from(value: $t) -> Self {
                Self {
                    inner: Arc::new(Some(Arc::new(value)).into()),
                }
            }
        }
    };
}

define_container!(ClientOptionsContainer, ClientOptions);
define_container!(ServerOptionsContainer, ServerOptions);
