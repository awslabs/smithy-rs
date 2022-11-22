/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use aws_smithy_xml::decode::{try_data, Document, ScopedDecoder, XmlDecodeError};
use std::convert::TryFrom;

#[allow(unused)]
pub fn body_is_error(body: &[u8]) -> Result<bool, XmlDecodeError> {
    let mut doc = Document::try_from(body)?;
    let scoped = doc.root_element()?;
    Ok(scoped.start_el().matches("ErrorResponse"))
}

pub fn parse_generic_error(body: &[u8]) -> Result<aws_smithy_types::Error, XmlDecodeError> {
    let mut doc = Document::try_from(body)?;
    let mut root = doc.root_element()?;
    let mut err_builder = aws_smithy_types::Error::builder();
    while let Some(mut tag) = root.next_tag() {
        match tag.start_el().local() {
            "Error" => {
                while let Some(mut error_field) = tag.next_tag() {
                    match error_field.start_el().local() {
                        "Code" => {
                            err_builder.code(try_data(&mut error_field)?);
                        }
                        "Message" => {
                            err_builder.message(try_data(&mut error_field)?);
                        }
                        _ => {}
                    }
                }
            }
            "RequestId" => {
                err_builder.request_id(try_data(&mut tag)?);
            }
            _ => {}
        }
    }
    Ok(err_builder.build())
}

#[allow(unused)]
pub fn error_scope<'a, 'b>(
    doc: &'a mut Document<'b>,
) -> Result<ScopedDecoder<'b, 'a>, XmlDecodeError> {
    let root = doc
        .next_start_element()
        .ok_or_else(|| XmlDecodeError::custom("no root found searching for an Error"))?;
    if !root.matches("ErrorResponse") {
        return Err(XmlDecodeError::custom("expected ErrorResponse as root"));
    }

    while let Some(el) = doc.next_start_element() {
        if el.matches("Error") && el.depth() == 1 {
            return Ok(doc.scoped_to(el));
        }
        // otherwise, ignore it
    }
    Err(XmlDecodeError::custom(
        "no error found inside of ErrorResponse",
    ))
}

#[cfg(test)]
mod test {
    use super::{body_is_error, parse_generic_error};
    use crate::rest_xml_wrapped_errors::error_scope;
    use aws_smithy_xml::decode::Document;
    use std::convert::TryFrom;

    #[test]
    fn parse_wrapped_error() {
        let xml = br#"<ErrorResponse>
    <Error>
        <Type>Sender</Type>
        <Code>InvalidGreeting</Code>
        <Message>Hi</Message>
        <AnotherSetting>setting</AnotherSetting>
        <Ignore><This/></Ignore>
    </Error>
    <RequestId>foo-id</RequestId>
</ErrorResponse>"#;
        assert!(body_is_error(xml).unwrap());
        let parsed = parse_generic_error(xml).expect("valid xml");
        assert_eq!(parsed.request_id(), Some("foo-id"));
        assert_eq!(parsed.message(), Some("Hi"));
        assert_eq!(parsed.code(), Some("InvalidGreeting"));
    }

    #[test]
    fn test_error_scope() {
        let xml: &[u8] = br#"<ErrorResponse>
    <RequestId>foo-id</RequestId>
    <MorePreamble>foo-id</RequestId>
    <Sneaky><Error>These are not the errors you are looking for</Error></Sneaky>
    <Error>
        <Type>Sender</Type>
        <Code>InvalidGreeting</Code>
        <Message>Hi</Message>
        <AnotherSetting>setting</AnotherSetting>
        <Ignore><This/></Ignore>
    </Error>
    <RequestId>foo-id</RequestId>
</ErrorResponse>"#;
        let mut doc = Document::try_from(xml).expect("valid");
        let mut error = error_scope(&mut doc).expect("contains error");
        let mut keys = vec![];
        while let Some(tag) = error.next_tag() {
            keys.push(tag.start_el().local().to_owned());
            // read this the full contents of this element
        }
        assert_eq!(
            keys,
            vec!["Type", "Code", "Message", "AnotherSetting", "Ignore",]
        )
    }
}
