use std::collections::HashMap;

use syn::{Lit, Path};

/// Gets the options specified for an element given all its attributes.
/// These will specified with annotations of the form:
///   #[arg(a = b, c = "d")]
pub fn get_options(attr_name: &str, attrs: &[syn::Attribute]) -> HashMap<Path, Option<Lit>> {
    let mut meta_map = HashMap::new();

    let arg_attr = attrs.iter().find(|attr| attr.path.is_ident(attr_name));
    if let Some(attr) = arg_attr {
        let meta = attr.parse_meta().unwrap();

        let meta_list = match meta {
            syn::Meta::List(list) => list.nested,
            _ => panic!("Unexpected arg attr format"),
        };

        for meta_item in meta_list {
            let name_value = match meta_item {
                syn::NestedMeta::Meta(syn::Meta::NameValue(v)) => v,
                syn::NestedMeta::Meta(syn::Meta::Path(p)) => {
                    meta_map.insert(p, None);
                    continue;
                }
                _ => panic!("Unsupported meta_item: {:?}", meta_item),
            };

            meta_map.insert(name_value.path, Some(name_value.lit));
        }
    }

    meta_map
}
