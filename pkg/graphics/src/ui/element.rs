use core::any::{Any, TypeId};
use std::sync::Arc;

use common::errors::*;

use crate::ui::view::{View, ViewParams, ViewWithParams};

use super::virtual_view::VirtualViewParams;

#[derive(Clone)]
pub struct Element {
    pub inner: Arc<dyn ElementInterface>,
}

pub trait ElementInterface: 'static {
    fn key(&self) -> (TypeId, &str);

    fn instantiate(&self) -> Result<Box<dyn View>>;

    fn as_any<'a>(&'a self) -> &'a dyn Any;

    // fn as_any(&self) -> &dyn Any
    // where
    //     Self: Sized,
    // {
    //     self
    // }

    // fn downcast_ref<T: ElementInterface>(&self) -> Result<&T> {
    //     Any::downcast_ref::<T>(self)
    // }
}

pub struct ViewWithParamsElement<V: 'static + ViewWithParams> {
    params: V::Params,
}

impl<V: 'static + ViewWithParams> ViewWithParamsElement<V> {
    pub fn new(params: V::Params) -> Self {
        Self { params }
    }

    pub fn params(&self) -> &V::Params {
        &self.params
    }
}

impl<V: 'static + ViewWithParams> ElementInterface for ViewWithParamsElement<V> {
    fn key(&self) -> (TypeId, &str) {
        (TypeId::of::<V>(), "")
    }

    fn instantiate(&self) -> Result<Box<dyn View>> {
        V::create_with_params(&self.params)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl<Params: ViewParams> From<Params> for Element {
    fn from(params: Params) -> Self {
        Self {
            inner: Arc::new(ViewWithParamsElement::<Params::View>::new(params)),
        }
    }
}
