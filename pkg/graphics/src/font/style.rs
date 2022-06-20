
#[derive(Clone, Debug)]
pub struct FontStyle {
    pub size: f32,
    pub text_align: TextAlign,
    pub vertical_align: VerticalAlign,
}

impl FontStyle {
    pub fn from_size(size: f32) -> Self {
        Self { size, text_align: TextAlign::Left, vertical_align: VerticalAlign::Baseline }
    }

    pub fn with_text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = text_align;
        self
    }

    pub fn with_vertical_align(mut self, vertical_align: VerticalAlign) -> Self {
        self.vertical_align = vertical_align;
        self
    }
}


#[derive(Clone, Debug)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug)]
pub enum VerticalAlign {
    Top,
    Baseline,
    Bottom,
    Center,
}