/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.generators

import io.kotest.matchers.shouldBe
import org.junit.jupiter.api.Test
import software.amazon.smithy.model.shapes.OperationShape
import software.amazon.smithy.model.shapes.ShapeId
import software.amazon.smithy.model.shapes.StructureShape
import software.amazon.smithy.model.traits.HttpTrait
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.rustBlock
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.generators.HttpTraitBindingGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.uriFormatString
import software.amazon.smithy.rust.codegen.smithy.transformers.OperationNormalizer
import software.amazon.smithy.rust.codegen.testutil.TestRuntimeConfig
import software.amazon.smithy.rust.codegen.testutil.asSmithyModel
import software.amazon.smithy.rust.codegen.testutil.compileAndTest
import software.amazon.smithy.rust.codegen.testutil.renderWithModelBuilder
import software.amazon.smithy.rust.codegen.testutil.testSymbolProvider
import software.amazon.smithy.rust.codegen.util.dq

class HttpTraitBindingGeneratorTest {
    private val baseModel = """
            namespace smithy.example

            @idempotent
            @http(method: "PUT", uri: "/{bucketName}/{key}", code: 200)
            operation PutObject {
                input: PutObjectRequest
            }

            list Extras {
                member: Integer
            }

            list Dates {
                member: Timestamp
            }

            @mediaType("video/quicktime")
            string Video

            structure PutObjectRequest {
                // Sent in the URI label named "key".
                @required
                @httpLabel
                key: Timestamp,

                // Sent in the URI label named "bucketName".
                @required
                @httpLabel
                bucketName: String,

                // Sent in the X-Dates header
                @httpHeader("X-Dates")
                dateHeaderList: Dates,

                @httpHeader("X-Ints")
                intList: Extras,

                @httpHeader("X-MediaType")
                mediaType: Video,

                // Sent in the query string as paramName
                @httpQuery("paramName")
                someValue: String,

                @httpQuery("primitive")
                primitive: PrimitiveInteger,

                @httpQuery("enabled")
                enabled: PrimitiveBoolean,

                @httpQuery("hello")
                extras: Extras,

                // Sent in the body
                data: Blob,

                // Sent in the body
                additional: String,
            }
        """.asSmithyModel()
    private val model = OperationNormalizer(baseModel).transformModel(
        inputBodyFactory = OperationNormalizer.NoBody,
        outputBodyFactory = OperationNormalizer.NoBody
    )

    private val operationShape = model.expectShape(ShapeId.from("smithy.example#PutObject"), OperationShape::class.java)
    private val inputShape = model.expectShape(operationShape.input.get(), StructureShape::class.java)
    private val httpTrait = operationShape.expectTrait(HttpTrait::class.java)

    private val symbolProvider = testSymbolProvider(model)
    private fun renderOperation(writer: RustWriter) {
        inputShape.renderWithModelBuilder(model, symbolProvider, writer)
        val inputShape = model.expectShape(operationShape.input.get(), StructureShape::class.java)
        writer.rustBlock("impl PutObjectInput") {
            HttpTraitBindingGenerator(
                model,
                symbolProvider,
                TestRuntimeConfig, writer, operationShape, inputShape, httpTrait
            ).renderUpdateHttpBuilder(this)
            rustBlock("pub fn request_builder_base(&self) -> #T", RuntimeType.HttpRequestBuilder) {
                write("let builder = #T::new();", RuntimeType.HttpRequestBuilder)
                write("self.update_http_builder(builder)")
            }
        }
    }

    @Test
    fun `produce correct uri format strings`() {
        httpTrait.uriFormatString() shouldBe ("/{bucketName}/{key}".dq())
    }

    @Test
    fun `generate uris`() {
        val writer = RustWriter.forModule("input")
        // currently rendering the operation renders the protocols—I want to separate that at some point.
        renderOperation(writer)
        writer.compileAndTest(
            """
            let ts = smithy_types::Instant::from_epoch_seconds(10123125);
            let inp = PutObjectInput::builder()
                .bucket_name("somebucket/ok")
                .key(ts.clone())
                .set_extras(vec![0,1,2,44])
                .some_value("svq!!%&")
                .build().expect("build should succeed");
            let mut o = String::new();
            inp.uri_base(&mut o);
            assert_eq!(o.as_str(), "/somebucket%2Fok/1970-04-28T03%3A58%3A45Z");
            o.clear();
            inp.uri_query(&mut o);
            assert_eq!(o.as_str(), "?paramName=svq!!%25%26&hello=0&hello=1&hello=2&hello=44")
            """
        )
    }

    @Test
    fun `generate serialize non-zero values`() {
        val writer = RustWriter.forModule("input")
        // currently rendering the operation renders the protocols—I want to separate that at some point.
        renderOperation(writer)
        writer.compileAndTest(
            """
            let ts = smithy_types::Instant::from_epoch_seconds(10123125);
            let inp = PutObjectInput::builder()
                .bucket_name("somebucket/ok")
                .key(ts.clone())
                .primitive(1)
                .enabled(true)
                .build().expect("build should succeed");
            let mut o = String::new();
            inp.uri_query(&mut o);
            assert_eq!(o.as_str(), "?primitive=1&enabled=true")
            """,
            clippy = true
        )
    }

    @Test
    fun `build http requests`() {
        val writer = RustWriter.forModule("input")
        renderOperation(writer)
        writer.compileAndTest(
            """
            let ts = smithy_types::Instant::from_epoch_seconds(10123125);
            let inp = PutObjectInput::builder()
                .bucket_name("buk")
                .set_date_header_list(vec![ts.clone()])
                .set_int_list(vec![0,1,44])
                .key(ts.clone())
                .set_extras(vec![0,1])
                .some_value("qp")
                .media_type("base64encodethis")
                .build().unwrap();
            let http_request = inp.request_builder_base().body(()).unwrap();
            assert_eq!(http_request.uri(), "/buk/1970-04-28T03%3A58%3A45Z?paramName=qp&hello=0&hello=1");
            assert_eq!(http_request.method(), "PUT");
            let mut date_header = http_request.headers().get_all("X-Dates").iter();
            assert_eq!(date_header.next().unwrap(), "Tue, 28 Apr 1970 03:58:45 GMT");
            assert_eq!(date_header.next(), None);

            let int_header = http_request.headers().get_all("X-Ints").iter().map(|hv|hv.to_str().unwrap()).collect::<Vec<_>>();
            assert_eq!(int_header, vec!["0", "1", "44"]);

            let base64_header = http_request.headers().get_all("X-MediaType").iter().map(|hv|hv.to_str().unwrap()).collect::<Vec<_>>();
            assert_eq!(base64_header, vec!["YmFzZTY0ZW5jb2RldGhpcw=="]);
        """
        )
    }
}
