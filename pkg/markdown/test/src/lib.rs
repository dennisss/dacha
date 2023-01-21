extern crate markdown;
#[macro_use]
extern crate macros;
extern crate common;

use common::errors::*;

#[testcase]
async fn commonmark_test() -> Result<()> {
    let mut data = file::read_to_string(file::project_path!(
        "third_party/commonmark-spec/tests.json"
    ))
    .await?;

    let value = json::parse(&data)?;

    let array = value.get_elements().unwrap();

    let mut num_passed = 0;
    let mut num_total = 0;

    for test_case in array {
        let section = test_case
            .get_field("section")
            .and_then(|v| v.get_string())
            .unwrap();
        let example_num = test_case
            .get_field("example")
            .and_then(|v| v.get_number())
            .unwrap();

        let md = test_case
            .get_field("markdown")
            .and_then(|v| v.get_string())
            .unwrap();
        let html = test_case
            .get_field("html")
            .and_then(|v| v.get_string())
            .unwrap();

        let md_parsed = markdown::Block::parse_document(md);

        let generated_html = md_parsed.to_html();

        let passing = generated_html == html;

        if !passing {
            println!("==== {}: {} ====", section, example_num);
        }

        if generated_html == html {
            num_passed += 1;
            // println!(":: Pass!");
        } else {
            println!(":: Failed!");
            println!("Markdown: {:?}", md);
            println!("Expected HTML: {:?}", html);
            println!("Actual   HTML: {:?}", generated_html);
        }

        num_total += 1;
    }

    println!("Result {} / {} passed", num_passed, num_total);

    Ok(())
}
