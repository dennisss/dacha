use std::rc::Rc;
use std::sync::{Arc, Mutex};

use common::errors::*;
use image::Color;

use crate::font::CanvasFontRenderer;
use crate::ui::box_view::BoxViewParams;
use crate::ui::button::ButtonParams;
use crate::ui::checkbox::CheckboxParams;
use crate::ui::element::*;
use crate::ui::grid_view::{GridDimensionSize, GridViewParams};
use crate::ui::text_view::TextViewParams;
use crate::ui::textbox::TextboxParams;
use crate::ui::virtual_view::*;

#[derive(Clone)]
pub struct ShoppingList {
    pub font: Rc<CanvasFontRenderer>,
}

pub struct ShoppingListView {
    params: ShoppingList,
    state: Arc<Mutex<ShoppingListState>>,
}

struct ShoppingListState {
    items: Vec<ShoppingListItem>,
}

struct ShoppingListItem {
    name: String,
    price: f32,
}

impl VirtualViewParams for ShoppingList {
    type View = ShoppingListView;
}

impl ShoppingListView {
    fn get_on_name_changed(&self, item_index: usize) -> Arc<dyn Fn(String)> {
        let state = self.state.clone();

        Arc::new(move |new_value| {
            state.lock().unwrap().items[item_index].name = new_value;
        })
    }

    fn get_add_item(&self) -> Arc<dyn Fn()> {
        let state = self.state.clone();

        Arc::new(move || {
            state.lock().unwrap().items.push(ShoppingListItem {
                name: String::new(),
                price: 0.,
            });
        })
    }
}

impl VirtualView for ShoppingListView {
    type Params = ShoppingList;

    fn create_with_params(params: &Self::Params) -> Result<Self> {
        let state = Arc::new(Mutex::new(ShoppingListState { items: vec![] }));

        Ok(Self {
            params: params.clone(),
            state,
        })
    }

    fn update_with_params(&mut self, params: &Self::Params) -> Result<()> {
        self.params = params.clone();
        Ok(())
    }

    fn build_element(&mut self) -> Result<Element> {
        let state = self.state.lock().unwrap();

        let mut els: Vec<Element> = vec![];

        for (i, item) in state.items.iter().enumerate() {
            els.push(
                BoxViewParams {
                    inner: CheckboxParams {
                        value: false,
                        on_change: None,
                    }
                    .into(),
                    // TODO: Integrate the padding into the checkbox.
                    padding: 8., // 0.6em at 16px font.
                    background_color: None,
                    border: None,
                    cursor: None,
                }
                .into(),
            );
            els.push(
                BoxViewParams {
                    inner: TextboxParams {
                        value: item.name.clone(),
                        font: self.params.font.clone(),
                        font_size: 16.,
                        on_change: Some(self.get_on_name_changed(i)),
                    }
                    .into(),
                    padding: 10.,
                    background_color: None,
                    border: None,
                    cursor: None,
                }
                .into(),
            );
        }

        Ok(GridViewParams {
            rows: vec![GridDimensionSize::FitContent, GridDimensionSize::FitContent],
            cols: vec![GridDimensionSize::Grow(1.)],
            children: vec![
                GridViewParams {
                    rows: vec![GridDimensionSize::FitContent; state.items.len()],
                    cols: vec![GridDimensionSize::FitContent, GridDimensionSize::Grow(1.)],
                    children: els,
                }
                .into(),
                ButtonParams {
                    inner: TextViewParams {
                        text: "Add Item".into(),
                        font: self.params.font.clone(),
                        font_size: 16.,
                        color: Color::rgb(255, 255, 255),
                    }
                    .into(),
                    on_click: Some(self.get_add_item()),
                }
                .into(),
            ],
        }
        .into())
    }
}
