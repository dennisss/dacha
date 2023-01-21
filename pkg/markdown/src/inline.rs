use crate::encoding::encode_html_text;

#[derive(Clone, Debug, Default)]
pub struct InlineContent {
    pub elements: Vec<InlineElement>,
}

impl InlineContent {
    pub fn to_html(&self) -> String {
        let mut inner = String::new();
        for el in &self.elements {
            inner.push_str(el.to_html().as_str());
        }

        inner
    }
}

/// Inline 'text like' elements that can appear in some Block types.
///
/// Note that string fields inside of this object have any backslash escaping
/// removed.
#[derive(Clone, Debug)]
pub enum InlineElement {
    Text(String),
    CodeSpan(String),
    Link {
        is_image: bool,
        text: InlineContent,
        link: String,
        title: Option<String>,
    },
    Emphasis {
        text: InlineContent,
        strong: bool,
    },
    SoftBreak,
    HardBreak,
}

impl InlineElement {
    pub fn to_html(&self) -> String {
        match self {
            InlineElement::Text(text) => encode_html_text(text),
            InlineElement::CodeSpan(code) => format!("<code>{}</code>", encode_html_text(code)),
            InlineElement::Link {
                is_image,
                text,
                link,
                title,
            } => {
                let title = title
                    .as_ref()
                    .map(|v| format!(" title=\"{}\"", encode_html_text(v)))
                    .unwrap_or("".into());

                let link = encode_html_text(link);

                if *is_image {
                    format!(
                        "<img src=\"{}\" alt=\"{}\"{} />",
                        link,
                        encode_html_text(&text.to_html()),
                        title
                    )
                } else {
                    format!("<a href=\"{}\"{}>{}</a>", link, title, text.to_html())
                }
            }
            InlineElement::Emphasis { text, strong } => {
                if *strong {
                    format!("<strong>{}</strong>", text.to_html())
                } else {
                    format!("<em>{}</em>", text.to_html())
                }
            }
            InlineElement::SoftBreak => "\n".into(),
            InlineElement::HardBreak => "<br />\n".into(),
        }
    }
}
