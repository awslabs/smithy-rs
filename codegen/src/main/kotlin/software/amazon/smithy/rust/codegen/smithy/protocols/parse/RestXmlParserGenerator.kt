/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.smithy.protocols.parse

import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.asType
import software.amazon.smithy.rust.codegen.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.rustlang.writable
import software.amazon.smithy.rust.codegen.smithy.CoreCodegenContext
import software.amazon.smithy.rust.codegen.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.smithy.protocols.AllowInvalidXmlRoot
import software.amazon.smithy.rust.codegen.smithy.traits.SyntheticOutputTrait
import software.amazon.smithy.rust.codegen.util.getTrait
import software.amazon.smithy.rust.codegen.util.hasTrait
import software.amazon.smithy.rust.codegen.util.orNull

class RestXmlParserGenerator(
    coreCodegenContext: CoreCodegenContext,
    xmlErrors: RuntimeType,
    private val xmlBindingTraitParserGenerator: XmlBindingTraitParserGenerator =
        XmlBindingTraitParserGenerator(
            coreCodegenContext,
            xmlErrors,
        ) { context, inner ->
            val shapeName = context.outputShapeName
            // Get the non-synthetic version of the outputShape and check to see if it has the `AllowInvalidXmlRoot` trait
            val allowInvalidRoot = context.model.getShape(context.shape.outputShape).orNull().let { shape ->
                shape?.getTrait<SyntheticOutputTrait>()?.originalId.let { shapeId ->
                    context.model.getShape(shapeId).orNull()?.hasTrait<AllowInvalidXmlRoot>() ?: false
                }
            }

            val invalidRootCheck = if (allowInvalidRoot) {
                writable {
                    rustTemplate(
                        """
                        // Zelda A
                        #{tracing}::trace!("legacy API returned invalid root, expected $shapeName but got {:?}", start_el)
                        """,
                        "tracing" to CargoDependency.Tracing.asType(),
                    )
                }
            } else {
                writable {
                    rustTemplate(
                        """
                        // Zelda B
                        return Err(
                            #{XmlError}::custom(format!("encountered invalid XML root: \
                                expected $shapeName but got {:?}. \
                                This is likely a bug in the SDK.", start_el))
                        )""",
                        "XmlError" to context.xmlErrorType,
                    )
                }
            }

            rustTemplate(
                """
                if !(${XmlBindingTraitParserGenerator.XmlName(shapeName).matchExpression("start_el")}) {
                    #{invalidRootCheck:W}
                }
                """,
                "invalidRootCheck" to invalidRootCheck,
            )
            inner("decoder")
        },
) : StructuredDataParserGenerator by xmlBindingTraitParserGenerator
