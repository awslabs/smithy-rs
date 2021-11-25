// Code generated by software.amazon.smithy.rust.codegen.smithy-rs. DO NOT EDIT.
pub fn deser_structure_crate_input_healthcheck_input(
    value: &[u8],
    #[allow(unused_mut)] mut builder: crate::input::healthcheck_input::Builder,
) -> Result<crate::input::healthcheck_input::Builder, aws_smithy_json::deserialize::Error> {
    let mut tokens_owned =
        aws_smithy_json::deserialize::json_token_iter(crate::json_deser::or_empty_doc(value))
            .peekable();
    let tokens = &mut tokens_owned;
    aws_smithy_json::deserialize::token::expect_start_object(tokens.next())?;
    aws_smithy_json::deserialize::token::skip_to_end(tokens)?;
    if tokens.next().is_some() {
        return Err(aws_smithy_json::deserialize::Error::custom(
            "found more JSON tokens after completing parsing",
        ));
    }
    Ok(builder)
}

pub fn deser_structure_crate_input_register_service_input(
    value: &[u8],
    mut builder: crate::input::register_service_input::Builder,
) -> Result<crate::input::register_service_input::Builder, aws_smithy_json::deserialize::Error> {
    let mut tokens_owned =
        aws_smithy_json::deserialize::json_token_iter(crate::json_deser::or_empty_doc(value))
            .peekable();
    let tokens = &mut tokens_owned;
    aws_smithy_json::deserialize::token::expect_start_object(tokens.next())?;
    loop {
        match tokens.next().transpose()? {
            Some(aws_smithy_json::deserialize::Token::EndObject { .. }) => break,
            Some(aws_smithy_json::deserialize::Token::ObjectKey { key, .. }) => {
                match key.to_unescaped()?.as_ref() {
                    "name" => {
                        builder = builder.set_name(
                            aws_smithy_json::deserialize::token::expect_string_or_null(
                                tokens.next(),
                            )?
                            .map(|s| s.to_unescaped().map(|u| u.into_owned()))
                            .transpose()?,
                        );
                    }
                    _ => aws_smithy_json::deserialize::token::skip_value(tokens)?,
                }
            }
            other => {
                return Err(aws_smithy_json::deserialize::Error::custom(format!(
                    "expected object key or end object, found: {:?}",
                    other
                )))
            }
        }
    }
    if tokens.next().is_some() {
        return Err(aws_smithy_json::deserialize::Error::custom(
            "found more JSON tokens after completing parsing",
        ));
    }
    Ok(builder)
}

pub fn or_empty_doc(data: &[u8]) -> &[u8] {
    if data.is_empty() {
        b"{}"
    } else {
        data
    }
}
