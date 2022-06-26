use std::rc::Rc;
use std::sync::{Arc, Mutex};

use common::errors::*;
use image::Color;

use crate::font::CanvasFontRenderer;
use crate::ui;
use crate::ui::grid::{GridDimensionSize, GridViewParams};
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

    fn build_element(&mut self) -> Result<ui::Element> {
        let state = self.state.lock().unwrap();

        let mut els: Vec<ui::Element> = vec![];

        els.push(
            ui::Block::new(ui::Checkbox {
                value: false,
                on_change: None,
            }.into())
            // TODO: Integrate the padding into the checkbox.
            .with_padding(8.) // 0.6em at 16px font.
            .into(),
        );

        els.push(
            ui::Block::new(ui::Paragraph {
                    children: vec![ui::Text {
                        value: "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum".into(),
                        font: self.params.font.clone(),
                        font_size: 16.,
                        color: Color::rgb(0, 0, 0),
                        on_change: None,
                        selectable: false,
                        editable: false,
                    }
                    .into()],
                }
                .into())
                .with_padding(10.)
            .into(),
        );

        for (i, item) in state.items.iter().enumerate() {
            els.push(
                ui::Block::new(ui::Checkbox {
                        value: false,
                        on_change: None,
                    }
                    .into())
                // TODO: Integrate the padding into the checkbox.
                .with_padding(8.) // 0.6em at 16px font.
                .into(),
            );
            els.push(
                ui::Block::new(ui::Textbox {
                    value: item.name.clone(),
                    font: self.params.font.clone(),
                    font_size: 16.,
                    on_change: Some(self.get_on_name_changed(i)),
                }.into())
                .with_padding(10.)
                .into(),
            );
        }

        Ok(GridViewParams {
            rows: vec![GridDimensionSize::FitContent, GridDimensionSize::FitContent],
            cols: vec![GridDimensionSize::Grow(1.)],
            children: vec![
                GridViewParams {
                    rows: vec![GridDimensionSize::FitContent; state.items.len() + 1],
                    cols: vec![GridDimensionSize::FitContent, GridDimensionSize::Grow(1.)],
                    children: els,
                }
                .into(),
                ui::Button {
                    inner: ui::Text {
                        value: "Add Item".into(),
                        font: self.params.font.clone(),
                        font_size: 16.,
                        color: Color::rgb(255, 255, 255),
                        on_change: None,
                        selectable: false,
                        editable: false,
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
