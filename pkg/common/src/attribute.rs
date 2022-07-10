pub trait AttributeTag {
    type Value;
}

pub trait GetAttributeValue<T: AttributeTag> {
    fn get_attr_value(&self) -> T::Value;
}

#[macro_export]
macro_rules! define_attr {
    ($tag:ident => $t:ty) => {
        pub struct $tag {
            _hidden: (),
        }

        impl $crate::attribute::AttributeTag for $tag {
            type Value = $t;
        }
    };
}

#[macro_export]
macro_rules! get_attr {
    ($obj:expr, $tag:ty) => {
        $crate::attribute::GetAttributeValue::<$tag>::get_attr_value($obj)
    };
}
