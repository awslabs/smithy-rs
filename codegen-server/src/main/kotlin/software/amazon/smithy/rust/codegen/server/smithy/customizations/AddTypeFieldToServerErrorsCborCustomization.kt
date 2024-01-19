/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.server.smithy.customizations

import software.amazon.smithy.model.traits.ErrorTrait
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.escape
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.protocols.serialize.CborSerializerCustomization
import software.amazon.smithy.rust.codegen.core.smithy.protocols.serialize.CborSerializerSection
import software.amazon.smithy.rust.codegen.core.util.hasTrait
import software.amazon.smithy.rust.codegen.server.smithy.ServerCodegenContext

/**
 * Smithy RPC v2 CBOR requires errors to be serialized in server responses with an additional `__type` field.
 */
class AddTypeFieldToServerErrorsCborCustomization : CborSerializerCustomization() {
    override fun section(section: CborSerializerSection): Writable = when (section) {
        is CborSerializerSection.BeforeSerializingStructureMembers ->
            if (section.structureShape.hasTrait<ErrorTrait>()) {
                writable {
                    rust(
                        """
                        ${section.encoderBindingName}
                            .str("__type")
                            .str("${escape(section.structureShape.id.toString())}");
                        """
                    )
                }
            } else {
                emptySection
            }
        else -> emptySection
    }
}
