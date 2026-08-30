//! Terraform's two syntaxes for one configuration.

use fun_refactor::transpile::tfjson;
use serde_json::Value;

fn as_json(hcl: &str) -> Value {
    serde_json::from_str(&tfjson::to_json(hcl).expect("json")).expect("valid json")
}

#[test]
fn a_block_header_becomes_one_level_of_nesting_per_label() {
    let json = as_json("resource \"aws_s3_bucket\" \"b\" {\n  acl = \"private\"\n}\n");
    assert_eq!(json["resource"]["aws_s3_bucket"]["b"]["acl"], "private");
}

#[test]
fn a_literal_crosses_as_the_kind_of_value_it_is() {
    // A number is a number and a bool is a bool.
    let json = as_json(
        "variable \"n\" {\n  default = 3\n  sensitive = true\n  description = \"how many\"\n  \
         fallback = null\n}\n",
    );
    let held = &json["variable"]["n"];
    assert_eq!(held["default"], 3);
    assert_eq!(held["sensitive"], true);
    assert_eq!(held["description"], "how many");
    assert_eq!(held["fallback"], Value::Null);
}

#[test]
fn an_expression_crosses_as_the_string_terraform_re_parses() {
    // `type = bool` names an expression Terraform reads, and the string "bool" is a different
    // thing.
    let json = as_json("variable \"n\" {\n  type = bool\n  count = var.many\n}\n");
    assert_eq!(json["variable"]["n"]["type"], "bool");
    assert_eq!(json["variable"]["n"]["count"], "var.many");
}

#[test]
fn a_nested_block_is_nesting() {
    let json =
        as_json("resource \"aws_s3_bucket\" \"b\" {\n  versioning {\n    enabled = true\n  }\n}\n");
    assert_eq!(
        json["resource"]["aws_s3_bucket"]["b"]["versioning"]["enabled"],
        true
    );
}

#[test]
fn two_blocks_under_one_header_share_a_level() {
    let json = as_json(
        "resource \"aws_s3_bucket\" \"one\" {\n  acl = \"private\"\n}\n\n\
         resource \"aws_s3_bucket\" \"two\" {\n  acl = \"public\"\n}\n",
    );
    let held = &json["resource"]["aws_s3_bucket"];
    assert_eq!(held["one"]["acl"], "private");
    assert_eq!(held["two"]["acl"], "public");
}

#[test]
fn the_json_comes_back_as_the_hcl_it_was() {
    // The round trip is the whole promise.
    let hcl = "resource \"aws_s3_bucket\" \"b\" {\n  acl = \"private\"\n  count = 2\n}\n";
    let json = tfjson::to_json(hcl).expect("json");
    let back = tfjson::to_hcl(&json).expect("hcl");
    let again = tfjson::to_json(&back).expect("json again");
    assert_eq!(
        serde_json::from_str::<Value>(&json).unwrap(),
        serde_json::from_str::<Value>(&again).unwrap(),
        "left:\n{json}\nhcl:\n{back}\nright:\n{again}"
    );
}

#[test]
fn an_expression_comes_back_as_an_expression() {
    // `"var.many"` in the JSON is what an author wrote as `var.many`.
    let json = "{\"variable\": {\"n\": {\"type\": \"bool\", \"count\": \"var.many\"}}}";
    let hcl = tfjson::to_hcl(json).expect("hcl");
    assert!(hcl.contains("type = bool"), "{hcl}");
    assert!(hcl.contains("count = var.many"), "{hcl}");
    // And a string that is a string keeps its quotes.
    let text =
        tfjson::to_hcl("{\"variable\": {\"n\": {\"description\": \"how many\"}}}").expect("hcl");
    assert!(text.contains("description = \"how many\""), "{text}");
}

#[test]
fn a_bare_word_that_is_not_a_reference_keeps_its_quotes() {
    // `acl = "private"` and `type = bool` look identical in JSON.
    let hcl =
        tfjson::to_hcl("{\"resource\": {\"aws_s3_bucket\": {\"b\": {\"acl\": \"private\"}}}}")
            .expect("hcl");
    assert!(hcl.contains("acl = \"private\""), "{hcl}");

    // And a reference stays bare, because quoting one changes what the
    // configuration means.
    let reference =
        tfjson::to_hcl("{\"output\": {\"o\": {\"value\": \"var.many\"}}}").expect("hcl");
    assert!(reference.contains("value = var.many"), "{reference}");

    // A hostname is not a reference, whatever its dots look like.
    let host = tfjson::to_hcl("{\"output\": {\"o\": {\"value\": \"example.com\"}}}").expect("hcl");
    assert!(host.contains("value = \"example.com\""), "{host}");
}

#[test]
fn a_string_that_is_a_string_survives_the_whole_round_trip() {
    // The round trip through JSON is blind to the quoting question on its own: an expression
    // and a string both arrive as strings.
    let hcl = "resource \"aws_s3_bucket\" \"b\" {\n  acl = \"private\"\n}\n";
    let back = tfjson::to_hcl(&tfjson::to_json(hcl).expect("json")).expect("hcl");
    assert!(back.contains("acl = \"private\""), "{back}");
}

#[test]
fn hcl_that_does_not_parse_is_refused_and_says_so() {
    let refused = tfjson::to_json("resource \"a\" {\n  acl = \n").expect_err("a refusal");
    assert!(refused.to_string().contains("does not parse"), "{refused}");
}

#[test]
fn json_that_is_not_a_configuration_is_refused() {
    let refused = tfjson::to_hcl("[1, 2, 3]").expect_err("a refusal");
    assert!(
        refused.to_string().contains("object at the top"),
        "{refused}"
    );
}
