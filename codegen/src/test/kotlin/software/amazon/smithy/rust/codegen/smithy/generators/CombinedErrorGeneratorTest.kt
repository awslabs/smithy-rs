/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import org.junit.jupiter.api.Test
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.testutil.compileAndTest
import software.amazon.smithy.rust.codegen.testutil.renderWithModelBuilder
import software.amazon.smithy.rust.codegen.testutil.testSymbolProvider
import software.amazon.smithy.rust.codegen.util.lookup

internal class CombinedErrorGeneratorTest {
    @Test
    fun `generate combined error enums`() {
        val model = """
        namespace error
        operation Greeting {
            errors: [InvalidGreeting, ComplexError, FooError]
        }

        @error("client")
        @retryable
        structure InvalidGreeting {
            message: String,
        }

        @error("server")
        structure FooError {}

        @error("server")
        structure ComplexError {
            abc: String,
            other: Integer
        }
        """.asSmithyModel()
        val symbolProvider = testSymbolProvider(model)
        val writer = RustWriter.forModule("error")
        listOf("FooError", "ComplexError", "InvalidGreeting").forEach {
            model.lookup<StructureShape>("error#$it").renderWithModelBuilder(model, symbolProvider, writer)
        }

        val generator = CombinedErrorGenerator(model, testSymbolProvider(model), model.lookup("error#Greeting"))
        generator.render(writer)

        writer.compileAndTest(
            """
            let kind = GreetingErrorKind::InvalidGreeting(InvalidGreeting::builder().message("an error").build());
            let error = GreetingError { kind, meta: smithy_types::Error { code: None, message: Some("an error".to_string()), request_id: None }};
            assert_eq!(format!("{}", error), "InvalidGreeting: an error");
            assert_eq!(error.message(), Some("an error"));
            assert_eq!(error.code(), Some("InvalidGreeting"));
            use smithy_types::retry::ProvideErrorKind;
            assert_eq!(error.retryable_error_kind(), Some(smithy_types::retry::ErrorKind::ClientError));


            // unhandled variants properly delegate message
            let error = GreetingError::generic(smithy_types::Error {
               code: None,
               message: Some("hello".to_string()),
               request_id: None
            });
            assert_eq!(error.message(), Some("hello"));

            let error = GreetingError::unhandled("some other error");
            assert_eq!(error.message(), None);
            assert_eq!(error.code(), None);
        """
        )
    }
}
