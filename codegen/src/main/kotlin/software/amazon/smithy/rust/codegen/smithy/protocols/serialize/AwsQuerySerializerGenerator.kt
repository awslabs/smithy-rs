/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.protocols.serialize

import software.amazon.smithy.model.shapes.MemberShape
import software.amazon.smithy.model.traits.XmlFlattenedTrait
import software.amazon.smithy.model.traits.XmlNameTrait
import software.amazon.smithy.rust.codegen.smithy.generators.ProtocolConfig
import software.amazon.smithy.rust.codegen.util.getTrait

class AwsQuerySerializerGenerator(protocolConfig: ProtocolConfig) : QuerySerializerGenerator(protocolConfig) {
    override val protocolName: String get() = "AWS Query"

    override fun MemberShape.queryKeyName(prioritizedFallback: String?): String =
        getTrait<XmlNameTrait>()?.value ?: memberName

    override fun MemberShape.isFlattened(): Boolean = getTrait<XmlFlattenedTrait>() != null
}
