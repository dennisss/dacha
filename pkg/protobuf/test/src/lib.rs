#[cfg(test)]
mod test {
    use protobuf::text::*;
    use protobuf::*;
    use protobuf_test_proto::*;

    #[test]
    fn generated_code_usage() {
        let mut list = ShoppingList::default();

        assert_eq!(list.id(), 0);
        assert_eq!(list.items_len(), 0);
        assert_eq!(list.store(), ShoppingList_Store::UNKNOWN);

        // A protobuf with all default fields should have no custom fields.
        assert_eq!(&list.serialize().unwrap(), &[]);

        list.set_id(0);
        list.set_name("".to_string());
        assert_eq!(&list.serialize().unwrap(), &[]);

        list.set_id(4);
        assert_eq!(&list.serialize().unwrap(), &[0x10, 4]);
    }

    // TODO: Add a check to verify that a repeated field containing default values
    // (e.g. "") serializes correctly.

    #[test]
    fn test_format_parsing_test() {
        let mut list = ShoppingList::default();
        parse_text_proto(
            r#"
            # This is a comment
            name: "Groceries"
            id: 3
            cost: 12.50
            items: [
                # And here is another
                {
                    name: "First"
                    fruit_type: ORANGES
                },
                <
                    name: "Second",
                    fruit_type: APPLES
                >
            ]
            store: WALMART
            items {
                fruit_type: BERRIES
                name: 'Third'
            }
            "#,
            &mut list,
        )
        .unwrap();

        assert_eq!(list.name(), "Groceries");
        assert_eq!(list.id(), 3);
        assert_eq!(list.cost(), 12.5);
        assert_eq!(list.store(), ShoppingList_Store::WALMART);

        assert_eq!(list.items().len(), 3);

        println!("{:?}", list);
    }

    #[test]
    fn test_format_serialize_test() {
        let mut list = ShoppingList::default();
        assert_eq!(serialize_text_proto(&list), "");

        list.set_id(123);
        assert_eq!(serialize_text_proto(&list), "id: 123\n");

        list.set_name("Hi there!");
        assert_eq!(
            serialize_text_proto(&list),
            "name: \"Hi there!\"\nid: 123\n"
        );
    }
}
