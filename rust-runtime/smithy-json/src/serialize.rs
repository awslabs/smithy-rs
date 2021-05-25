/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

use crate::escape::escape_string;
use smithy_types::instant::Format;
use smithy_types::{Document, Instant, Number};
use std::borrow::Cow;

pub struct JsonValueWriter<'a> {
    output: &'a mut String,
}

impl<'a> JsonValueWriter<'a> {
    pub fn new(output: &'a mut String) -> Self {
        JsonValueWriter { output }
    }

    /// Writes a null value.
    pub fn null(self) {
        self.output.push_str("null");
    }

    /// Writes the boolean `value`.
    pub fn boolean(self, value: bool) {
        self.output.push_str(match value {
            true => "true",
            _ => "false",
        });
    }

    /// Writes a document `value`.
    pub fn document(self, value: &Document) {
        match value {
            Document::Array(values) => {
                let mut array = self.start_array();
                for value in values {
                    array.document(value);
                }
                array.finish();
            }
            Document::Bool(value) => self.boolean(*value),
            Document::Null => self.null(),
            Document::Number(value) => self.number(*value),
            Document::Object(values) => {
                let mut object = self.start_object();
                for (key, value) in values {
                    object.key(key).document(value);
                }
                object.finish();
            }
            Document::String(value) => self.string(&value),
        }
    }

    /// Writes a string `value`.
    pub fn string(self, value: &str) {
        self.output.push('"');
        self.output.push_str(&escape_string(value));
        self.output.push('"');
    }

    /// Writes a string `value` without escaping it.
    pub fn string_unchecked(self, value: &str) {
        // Verify in debug builds that we don't actually need to escape the string
        debug_assert!(matches!(escape_string(value), Cow::Borrowed(_)));

        self.output.push('"');
        self.output.push_str(value);
        self.output.push('"');
    }

    /// Writes a number `value`.
    pub fn number(self, value: Number) {
        match value {
            Number::PosInt(value) => {
                // itoa::Buffer is a fixed-size stack allocation, so this is cheap
                self.output.push_str(itoa::Buffer::new().format(value));
            }
            Number::NegInt(value) => {
                self.output.push_str(itoa::Buffer::new().format(value));
            }
            Number::Float(value) => {
                // If the value is NaN, Infinity, or -Infinity
                if value.is_nan() || value.is_infinite() {
                    self.output.push_str("null");
                } else {
                    // ryu::Buffer is a fixed-size stack allocation, so this is cheap
                    self.output
                        .push_str(ryu::Buffer::new().format_finite(value));
                }
            }
        }
    }

    /// Writes an Instant `value` with the given `format`.
    pub fn instant(self, instant: &Instant, format: Format) {
        let formatted = instant.fmt(format);
        match format {
            Format::EpochSeconds => self.output.push_str(&formatted),
            _ => self.string(&formatted),
        }
    }

    /// Starts an array.
    pub fn start_array(self) -> JsonArrayWriter<'a> {
        JsonArrayWriter::new(self.output)
    }

    /// Starts an object.
    pub fn start_object(self) -> JsonObjectWriter<'a> {
        JsonObjectWriter::new(self.output)
    }
}

pub struct JsonObjectWriter<'a> {
    json: &'a mut String,
    started: bool,
}

impl<'a> JsonObjectWriter<'a> {
    pub fn new(output: &'a mut String) -> Self {
        output.push('{');
        Self {
            json: output,
            started: false,
        }
    }

    /// Starts a value with the given `key`.
    pub fn key(&mut self, key: &str) -> JsonValueWriter {
        if self.started {
            self.json.push(',');
        }
        self.started = true;

        self.json.push('"');
        self.json.push_str(&escape_string(key));
        self.json.push_str("\":");

        JsonValueWriter::new(&mut self.json)
    }

    /// Finishes the object.
    pub fn finish(self) {
        self.json.push('}');
    }
}

pub struct JsonArrayWriter<'a> {
    json: &'a mut String,
    started: bool,
}

impl<'a> JsonArrayWriter<'a> {
    pub fn new(output: &'a mut String) -> Self {
        output.push('[');
        Self {
            json: output,
            started: false,
        }
    }

    #[inline]
    fn write<F: Fn(JsonValueWriter) -> ()>(&mut self, f: F) -> &mut Self {
        self.comma_delimit();
        f(JsonValueWriter::new(&mut self.json));
        self
    }

    /// Writes a null value to the array.
    pub fn null(&mut self) -> &mut Self {
        self.write(|w| w.null())
    }

    /// Writes the boolean `value` to the array.
    pub fn boolean(&mut self, value: bool) -> &mut Self {
        self.write(|w| w.boolean(value))
    }

    /// Writes a document `value`.
    pub fn document(&mut self, value: &Document) -> &mut Self {
        self.write(|w| w.document(value))
    }

    /// Writes a string to the array.
    pub fn string(&mut self, value: &str) -> &mut Self {
        self.write(|w| w.string(value))
    }

    /// Writes a string `value` to the array without escaping it.
    pub fn string_unchecked(&mut self, value: &str) -> &mut Self {
        self.write(|w| w.string_unchecked(value))
    }

    /// Writes a number `value` to the array.
    pub fn number(&mut self, value: Number) -> &mut Self {
        self.write(|w| w.number(value))
    }

    /// Writes an Instant `value` using `format` to the array.
    pub fn instant(&mut self, instant: &Instant, format: Format) -> &mut Self {
        self.write(|w| w.instant(instant, format))
    }

    /// Starts a nested array inside of the array.
    pub fn start_array(&mut self) -> JsonArrayWriter {
        self.comma_delimit();
        JsonArrayWriter::new(&mut self.json)
    }

    /// Starts a nested object inside of the array.
    pub fn start_object(&mut self) -> JsonObjectWriter {
        self.comma_delimit();
        JsonObjectWriter::new(&mut self.json)
    }

    /// Finishes the array.
    pub fn finish(self) {
        self.json.push(']');
    }

    fn comma_delimit(&mut self) {
        if self.started {
            self.json.push(',');
        }
        self.started = true;
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonArrayWriter, JsonObjectWriter};
    use crate::serialize::JsonValueWriter;
    use proptest::proptest;
    use smithy_types::instant::Format;
    use smithy_types::{Document, Instant, Number};

    #[test]
    fn empty() {
        let mut output = String::new();
        JsonObjectWriter::new(&mut output).finish();
        assert_eq!("{}", &output);

        let mut output = String::new();
        JsonArrayWriter::new(&mut output).finish();
        assert_eq!("[]", &output);
    }

    #[test]
    fn object_inside_array() {
        let mut output = String::new();
        let mut array = JsonArrayWriter::new(&mut output);
        array.start_object().finish();
        array.start_object().finish();
        array.start_object().finish();
        array.finish();
        assert_eq!("[{},{},{}]", &output);
    }

    #[test]
    fn object_inside_object() {
        let mut output = String::new();
        let mut obj_1 = JsonObjectWriter::new(&mut output);

        let mut obj_2 = obj_1.key("nested").start_object();
        obj_2.key("test").string("test");
        obj_2.finish();

        obj_1.finish();
        assert_eq!(r#"{"nested":{"test":"test"}}"#, &output);
    }

    #[test]
    fn array_inside_object() {
        let mut output = String::new();
        let mut object = JsonObjectWriter::new(&mut output);
        object.key("foo").start_array().finish();
        object.key("ba\nr").start_array().finish();
        object.finish();
        assert_eq!(r#"{"foo":[],"ba\nr":[]}"#, &output);
    }

    #[test]
    fn array_inside_array() {
        let mut output = String::new();

        let mut arr_1 = JsonArrayWriter::new(&mut output);

        let mut arr_2 = arr_1.start_array();
        arr_2.number(Number::PosInt(5));
        arr_2.finish();

        arr_1.start_array().finish();
        arr_1.finish();

        assert_eq!("[[5],[]]", &output);
    }

    #[test]
    fn object() {
        let mut output = String::new();
        let mut object = JsonObjectWriter::new(&mut output);
        object.key("true_val").boolean(true);
        object.key("false_val").boolean(false);
        object.key("some_string").string("some\nstring\nvalue");
        object.key("unchecked_str").string_unchecked("unchecked");
        object.key("some_number").number(Number::Float(3.5));
        object.key("some_null").null();

        let mut array = object.key("some_mixed_array").start_array();
        array
            .string("1")
            .number(Number::NegInt(-2))
            .string_unchecked("unchecked")
            .boolean(true)
            .boolean(false)
            .null();
        array.finish();

        object.finish();

        assert_eq!(
            r#"{"true_val":true,"false_val":false,"some_string":"some\nstring\nvalue","unchecked_str":"unchecked","some_number":3.5,"some_null":null,"some_mixed_array":["1",-2,"unchecked",true,false,null]}"#,
            &output
        );
    }

    #[test]
    fn object_instants() {
        let mut output = String::new();

        let mut object = JsonObjectWriter::new(&mut output);
        object
            .key("epoch_seconds")
            .instant(&Instant::from_f64(5.2), Format::EpochSeconds);
        object.key("date_time").instant(
            &Instant::from_str("2021-05-24T15:34:50.123Z", Format::DateTime).unwrap(),
            Format::DateTime,
        );
        object.key("http_date").instant(
            &Instant::from_str("Wed, 21 Oct 2015 07:28:00 GMT", Format::HttpDate).unwrap(),
            Format::HttpDate,
        );
        object.finish();

        assert_eq!(
            r#"{"epoch_seconds":5.2,"date_time":"2021-05-24T15:34:50.123Z","http_date":"Wed, 21 Oct 2015 07:28:00 GMT"}"#,
            &output,
        )
    }

    #[test]
    fn array_instants() {
        let mut output = String::new();

        let mut array = JsonArrayWriter::new(&mut output);
        array.instant(&Instant::from_f64(5.2), Format::EpochSeconds);
        array.instant(
            &Instant::from_str("2021-05-24T15:34:50.123Z", Format::DateTime).unwrap(),
            Format::DateTime,
        );
        array.instant(
            &Instant::from_str("Wed, 21 Oct 2015 07:28:00 GMT", Format::HttpDate).unwrap(),
            Format::HttpDate,
        );
        array.finish();

        assert_eq!(
            r#"[5.2,"2021-05-24T15:34:50.123Z","Wed, 21 Oct 2015 07:28:00 GMT"]"#,
            &output,
        )
    }

    fn format_document(document: Document) -> String {
        let mut output = String::new();
        JsonValueWriter::new(&mut output).document(&document);
        output
    }

    #[test]
    fn document() {
        assert_eq!("null", format_document(Document::Null));
        assert_eq!("true", format_document(Document::Bool(true)));
        assert_eq!("false", format_document(Document::Bool(false)));
        assert_eq!("5", format_document(Document::Number(Number::PosInt(5))));
        assert_eq!("\"test\"", format_document(Document::String("test".into())));
        assert_eq!(
            "[null,true,\"test\"]",
            format_document(Document::Array(vec![
                Document::Null,
                Document::Bool(true),
                Document::String("test".into())
            ]))
        );
        assert_eq!(
            r#"{"test":"foo"}"#,
            format_document(Document::Object(
                vec![("test".to_string(), Document::String("foo".into()))]
                    .into_iter()
                    .collect()
            ))
        );
        assert_eq!(
            r#"{"test1":[{"num":1},{"num":2}]}"#,
            format_document(Document::Object(
                vec![(
                    "test1".to_string(),
                    Document::Array(vec![
                        Document::Object(
                            vec![("num".to_string(), Document::Number(Number::PosInt(1))),]
                                .into_iter()
                                .collect()
                        ),
                        Document::Object(
                            vec![("num".to_string(), Document::Number(Number::PosInt(2))),]
                                .into_iter()
                                .collect()
                        ),
                    ])
                ),]
                .into_iter()
                .collect()
            ))
        );
    }

    fn format_test_number(number: Number) -> String {
        let mut formatted = String::new();
        JsonValueWriter::new(&mut formatted).number(number);
        formatted
    }

    #[test]
    fn number_formatting() {
        assert_eq!("1", format_test_number(Number::PosInt(1)));
        assert_eq!("-1", format_test_number(Number::NegInt(-1)));
        assert_eq!("1", format_test_number(Number::NegInt(1)));
        assert_eq!("0.0", format_test_number(Number::Float(0.0)));
        assert_eq!("10000000000.0", format_test_number(Number::Float(1e10)));
        assert_eq!("-1.2", format_test_number(Number::Float(-1.2)));

        // JSON doesn't support NaN, Infinity, or -Infinity, so we're matching
        // the behavior of the serde_json crate in these cases.
        assert_eq!(
            serde_json::to_string(&f64::NAN).unwrap(),
            format_test_number(Number::Float(f64::NAN))
        );
        assert_eq!(
            serde_json::to_string(&f64::INFINITY).unwrap(),
            format_test_number(Number::Float(f64::INFINITY))
        );
        assert_eq!(
            serde_json::to_string(&f64::NEG_INFINITY).unwrap(),
            format_test_number(Number::Float(f64::NEG_INFINITY))
        );
    }

    proptest! {
        #[test]
        fn matches_serde_json_pos_int_format(value: u64) {
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                format_test_number(Number::PosInt(value)),
            )
        }

        #[test]
        fn matches_serde_json_neg_int_format(value: i64) {
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                format_test_number(Number::NegInt(value)),
            )
        }

        #[test]
        fn matches_serde_json_float_format(value: f64) {
            assert_eq!(
                serde_json::to_string(&value).unwrap(),
                format_test_number(Number::Float(value)),
            )
        }
    }
}
