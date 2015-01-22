/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use std::io;
use std::io::{File, Command, Writer, TempDir, IoResult};
use std::num::Float;
use serialize::json::{self, Json, ToJson};
use test;

use encoding::label::encoding_from_whatwg_label;

use super::*;
use ast::*;
use ast::ComponentValue::*;

macro_rules! JString {
    ($e: expr) => { Json::String($e.to_string()) }
}

macro_rules! JArray {
    ($($e: expr),*) => { Json::Array(vec!( $($e),* )) }
}


fn write_whole_file(path: &Path, data: &str) -> IoResult<()> {
    (try!(File::open_mode(path, io::Open, io::Write))).write(data.as_bytes())
}


fn print_json_diff(results: &Json, expected: &Json) -> IoResult<()> {
    use std::io::stdio::stdout;

    let temp = try!(TempDir::new("rust-cssparser-tests"));
    let results = results.pretty().to_string() + "\n";
    let expected = expected.pretty().to_string() + "\n";
    let mut result_path = temp.path().clone();
    result_path.push("results.json");
    let mut expected_path = temp.path().clone();
    expected_path.push("expected.json");
    try!(write_whole_file(&result_path, results.as_slice()));
    try!(write_whole_file(&expected_path, expected.as_slice()));
    stdout().write(try!(Command::new("colordiff")
        .arg("-u1000")
        .arg(result_path.display().to_string())
        .arg(expected_path.display().to_string())
        .output()
        .map_err(|_| io::standard_error(io::OtherIoError))
    ).output.as_slice())
}


fn almost_equals(a: &Json, b: &Json) -> bool {
    match (a, b) {
        (&Json::I64(a), _) => almost_equals(&Json::F64(a as f64), b),
        (&Json::U64(a), _) => almost_equals(&Json::F64(a as f64), b),
        (_, &Json::I64(b)) => almost_equals(a, &Json::F64(b as f64)),
        (_, &Json::U64(b)) => almost_equals(a, &Json::F64(b as f64)),

        (&Json::F64(a), &Json::F64(b)) => (a - b).abs() < 1e-6,

        (&Json::Boolean(a), &Json::Boolean(b)) => a == b,
        (&Json::String(ref a), &Json::String(ref b)) => a == b,
        (&Json::Array(ref a), &Json::Array(ref b))
            => a.iter().zip(b.iter()).all(|(ref a, ref b)| almost_equals(*a, *b)),
        (&Json::Object(_), &Json::Object(_))
            => panic!("Not implemented"),
        (&Json::Null, &Json::Null) => true,
        _ => false,
    }
}


fn assert_json_eq(results: Json, expected: Json, message: String) {
    if !almost_equals(&results, &expected) {
        print_json_diff(&results, &expected).unwrap();
        panic!(message)
    }
}


fn run_raw_json_tests<F: Fn(Json, Json) -> ()>(json_data: &str, run: F) {
    let items = match json::from_str(json_data) {
        Ok(Json::Array(items)) => items,
        _ => panic!("Invalid JSON")
    };
    assert!(items.len() % 2 == 0);
    let mut input = None;
    for item in items.into_iter() {
        match (&input, item) {
            (&None, json_obj) => input = Some(json_obj),
            (&Some(_), expected) => {
                let input = input.take().unwrap();
                run(input, expected)
            },
        };
    }
}


fn run_json_tests<T: ToJson, F: Fn(&str) -> T>(json_data: &str, parse: F) {
    run_raw_json_tests(json_data, |input, expected| {
        match input {
            Json::String(input) => {
                let result = parse(input.as_slice()).to_json();
                assert_json_eq(result, expected, input);
            },
            _ => panic!("Unexpected JSON")
        }
    });
}


#[test]
fn component_value_list() {
    run_json_tests(include_str!("css-parsing-tests/component_value_list.json"), |input| {
        tokenize(input).map(|(c, _)| c).collect::<Vec<ComponentValue>>()
    });
}


#[test]
fn one_component_value() {
    run_json_tests(include_str!("css-parsing-tests/one_component_value.json"), |input| {
        parse_one_component_value(tokenize(input))
    });
}


#[test]
fn declaration_list() {
    run_json_tests(include_str!("css-parsing-tests/declaration_list.json"), |input| {
        parse_declaration_list(tokenize(input)).collect::<Vec<Result<DeclarationListItem, SyntaxError>>>()
    });
}


#[test]
fn one_declaration() {
    run_json_tests(include_str!("css-parsing-tests/one_declaration.json"), |input| {
        parse_one_declaration(tokenize(input))
    });
}


#[test]
fn rule_list() {
    run_json_tests(include_str!("css-parsing-tests/rule_list.json"), |input| {
        parse_rule_list(tokenize(input)).collect::<Vec<Result<Rule, SyntaxError>>>()
    });
}


#[test]
fn stylesheet() {
    run_json_tests(include_str!("css-parsing-tests/stylesheet.json"), |input| {
        parse_stylesheet_rules(tokenize(input)).collect::<Vec<Result<Rule, SyntaxError>>>()
    });
}


#[test]
fn one_rule() {
    run_json_tests(include_str!("css-parsing-tests/one_rule.json"), |input| {
        parse_one_rule(tokenize(input))
    });
}


#[test]
fn stylesheet_from_bytes() {
    run_raw_json_tests(include_str!("css-parsing-tests/stylesheet_bytes.json"),
    |input, expected| {
        let map = match input {
            Json::Object(map) => map,
            _ => panic!("Unexpected JSON")
        };

        let result = {
            let css = get_string(&map, "css_bytes").unwrap().chars().map(|c| {
                assert!(c as u32 <= 0xFF);
                c as u8
            }).collect::<Vec<u8>>();
            let protocol_encoding_label = get_string(&map, "protocol_encoding");
            let environment_encoding = get_string(&map, "environment_encoding")
                .and_then(encoding_from_whatwg_label);

            let (rules, used_encoding) = parse_stylesheet_rules_from_bytes(
                css.as_slice(), protocol_encoding_label, environment_encoding);

            (rules.collect::<Vec<_>>(), used_encoding.name().to_string()).to_json()
        };
        assert_json_eq(result, expected, Json::Object(map).to_string());
    });

    fn get_string<'a>(map: &'a json::Object, key: &str) -> Option<&'a str> {
        match map.get(key) {
            Some(&Json::String(ref s)) => Some(s.as_slice()),
            Some(&Json::Null) => None,
            None => None,
            _ => panic!("Unexpected JSON"),
        }
    }
}


fn run_color_tests<F: Fn(Option<Color>) -> Json>(json_data: &str, to_json: F) {
    run_json_tests(json_data, |input| {
        match parse_one_component_value(tokenize(input)) {
            Ok(component_value) => to_json(Color::parse(&component_value).ok()),
            Err(_reason) => Json::Null,
        }
    });
}


#[test]
fn color3() {
    run_color_tests(include_str!("css-parsing-tests/color3.json"), |c| c.to_json())
}


#[test]
fn color3_hsl() {
    run_color_tests(include_str!("css-parsing-tests/color3_hsl.json"), |c| c.to_json())
}


/// color3_keywords.json is different: R, G and B are in 0..255 rather than 0..1
#[test]
fn color3_keywords() {
    run_color_tests(include_str!("css-parsing-tests/color3_keywords.json"), |c| {
        match c {
            Some(Color::RGBA(RGBA { red: r, green: g, blue: b, alpha: a }))
            => vec!(r * 255., g * 255., b * 255., a).to_json(),
            Some(Color::CurrentColor) => JString!("currentColor"),
            None => Json::Null,
        }
    });
}


#[bench]
fn bench_color_lookup_red(b: &mut test::Bencher) {
    let ident = parse_one_component_value(tokenize("red")).unwrap();
    b.iter(|| assert!(Color::parse(&ident).is_ok()));
}


#[bench]
fn bench_color_lookup_lightgoldenrodyellow(b: &mut test::Bencher) {
    let ident = parse_one_component_value(tokenize("lightgoldenrodyellow")).unwrap();
    b.iter(|| assert!(Color::parse(&ident).is_ok()));
}


#[bench]
fn bench_color_lookup_fail(b: &mut test::Bencher) {
    let ident = parse_one_component_value(tokenize("lightgoldenrodyellowbazinga")).unwrap();
    b.iter(|| assert!(Color::parse(&ident).is_err()));
}


#[test]
fn nth() {
    run_json_tests(include_str!("css-parsing-tests/An+B.json"), |input| {
        parse_nth(tokenize(input).map(|(c, _)| c).collect::<Vec<ComponentValue>>().as_slice()).ok()
    });
}


#[test]
fn serializer() {
    run_json_tests(include_str!("css-parsing-tests/component_value_list.json"), |input| {
        let component_values = tokenize(input).map(|(c, _)| c).collect::<Vec<ComponentValue>>();
        let serialized = component_values.to_css_string();
        tokenize(serialized.as_slice()).map(|(c, _)| c).collect::<Vec<ComponentValue>>()
    });
}


#[test]
fn serialize_current_color() {
    let c = Color::CurrentColor;
    assert!(c.to_css_string() == "currentColor");
}


#[test]
fn serialize_rgb_full_alpha() {
    let c = Color::RGBA(RGBA { red: 1.0, green: 0.9, blue: 0.8, alpha: 1.0 });
    assert!(c.to_css_string() == "rgb(255, 230, 204)");
}


#[test]
fn serialize_rgba() {
    let c = Color::RGBA(RGBA { red: 0.1, green: 0.2, blue: 0.3, alpha: 0.5 });
    assert!(c.to_css_string() == "rgba(26, 51, 77, 0.5)");
}


impl ToJson for Result<Rule, SyntaxError> {
    fn to_json(&self) -> json::Json {
        match *self {
            Ok(ref a) => a.to_json(),
            Err(ref b) => b.to_json(),
        }
    }
}


impl ToJson for Result<DeclarationListItem, SyntaxError> {
    fn to_json(&self) -> json::Json {
        match *self {
            Ok(ref a) => a.to_json(),
            Err(ref b) => b.to_json(),
        }
    }
}


impl ToJson for Result<Declaration, SyntaxError> {
    fn to_json(&self) -> json::Json {
        match *self {
            Ok(ref a) => a.to_json(),
            Err(ref b) => b.to_json(),
        }
    }
}


impl ToJson for Result<ComponentValue, SyntaxError> {
    fn to_json(&self) -> json::Json {
        match *self {
            Ok(ref a) => a.to_json(),
            Err(ref b) => b.to_json(),
        }
    }
}


impl ToJson for SyntaxError {
    fn to_json(&self) -> json::Json {
        Json::Array(vec!(JString!("error"), JString!(match self.reason {
            ErrorReason::EmptyInput => "empty",
            ErrorReason::ExtraInput => "extra-input",
            _ => "invalid",
        })))
    }
}


impl ToJson for Color {
    fn to_json(&self) -> json::Json {
        match *self {
            Color::RGBA(RGBA { red: r, green: g, blue: b, alpha: a }) => vec!(r, g, b, a).to_json(),
            Color::CurrentColor => JString!("currentColor"),
        }
    }
}


impl ToJson for Rule {
    fn to_json(&self) -> json::Json {
        match *self {
            Rule::QualifiedRule(ref rule) => rule.to_json(),
            Rule::AtRule(ref rule) => rule.to_json(),
        }
    }
}


impl ToJson for DeclarationListItem {
    fn to_json(&self) -> json::Json {
        match *self {
            DeclarationListItem::Declaration(ref declaration) => declaration.to_json(),
            DeclarationListItem::AtRule(ref at_rule) => at_rule.to_json(),
        }
    }
}


fn list_to_json(list: &Vec<(ComponentValue, SourceLocation)>) -> Vec<json::Json> {
    list.iter().map(|tuple| {
        match *tuple {
            (ref c, _) => c.to_json()
        }
    }).collect()
}


impl ToJson for AtRule {
    fn to_json(&self) -> json::Json {
        match *self {
            AtRule{ ref name, ref prelude, ref block, ..}
            => Json::Array(vec!(JString!("at-rule"), name.to_json(),
                                prelude.to_json(), block.as_ref().map(list_to_json).to_json()))
        }
    }
}


impl ToJson for QualifiedRule {
    fn to_json(&self) -> json::Json {
        match *self {
            QualifiedRule{ ref prelude, ref block, ..}
            => Json::Array(vec!(JString!("qualified rule"),
                                prelude.to_json(), Json::Array(list_to_json(block))))
        }
    }
}


impl ToJson for Declaration {
    fn to_json(&self) -> json::Json {
        match *self {
            Declaration{ ref name, ref value, ref important, ..}
            =>  Json::Array(vec!(JString!("declaration"), name.to_json(),
                                 value.to_json(), important.to_json()))
        }
    }
}


impl ToJson for ComponentValue {
    fn to_json(&self) -> json::Json {
        fn numeric(value: &NumericValue) -> Vec<json::Json> {
            match *value {
                NumericValue{representation: ref r, value: ref v, int_value: ref i}
                => vec!(r.to_json(), v.to_json(),
                        JString!(match *i { Some(_) => "integer", _ => "number" }))
            }
        }

        match *self {
            Ident(ref value) => JArray!(JString!("ident"), value.to_json()),
            AtKeyword(ref value) => JArray!(JString!("at-keyword"), value.to_json()),
            Hash(ref value) => JArray!(JString!("hash"), value.to_json(),
                                      JString!("unrestricted")),
            IDHash(ref value) => JArray!(JString!("hash"), value.to_json(), JString!("id")),
            QuotedString(ref value) => JArray!(JString!("string"), value.to_json()),
            URL(ref value) => JArray!(JString!("url"), value.to_json()),
            Delim('\\') => JString!("\\"),
            Delim(value) => Json::String(value.to_string()),

            Number(ref value) => Json::Array(
                vec!(JString!("number")) + numeric(value).as_slice()),
            Percentage(ref value) => Json::Array(
                vec!(JString!("percentage")) + numeric(value).as_slice()),
            Dimension(ref value, ref unit) => Json::Array(
                vec!(JString!("dimension")) + numeric(value).as_slice()
                + [unit.to_json()].as_slice()),

            UnicodeRange(start, end)
            => JArray!(JString!("unicode-range"), start.to_json(), end.to_json()),

            WhiteSpace => JString!(" "),
            Colon => JString!(":"),
            Semicolon => JString!(";"),
            Comma => JString!(","),
            IncludeMatch => JString!("~="),
            DashMatch => JString!("|="),
            PrefixMatch => JString!("^="),
            SuffixMatch => JString!("$="),
            SubstringMatch => JString!("*="),
            Column => JString!("||"),
            CDO => JString!("<!--"),
            CDC => JString!("-->"),

            Function(ref name, ref arguments)
            => Json::Array(
                vec!(JString!("function"), name.to_json())
                + arguments.iter().map(|a| a.to_json()).collect::<Vec<json::Json>>().as_slice()),
            ParenthesisBlock(ref content)
            => Json::Array(
                vec!(JString!("()"))
                + content.iter().map(|c| c.to_json()).collect::<Vec<json::Json>>().as_slice()),
            SquareBracketBlock(ref content)
            => Json::Array(
                vec!(JString!("[]"))
                + content.iter().map(|c| c.to_json()).collect::<Vec<json::Json>>().as_slice()),
            CurlyBracketBlock(ref content)
            => Json::Array(vec!(JString!("{}")) + list_to_json(content).as_slice()),

            BadURL => JArray!(JString!("error"), JString!("bad-url")),
            BadString => JArray!(JString!("error"), JString!("bad-string")),
            CloseParenthesis => JArray!(JString!("error"), JString!(")")),
            CloseSquareBracket => JArray!(JString!("error"), JString!("]")),
            CloseCurlyBracket => JArray!(JString!("error"), JString!("}")),
        }
    }
}
