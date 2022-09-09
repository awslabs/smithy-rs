/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.smithy.customize

import io.kotest.matchers.collections.shouldContainExactly
import org.junit.jupiter.api.Test
import software.amazon.smithy.rust.codegen.client.smithy.ClientCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.ServerCodegenContext
import software.amazon.smithy.rust.codegen.client.smithy.customize.CombinedCodegenDecorator
import software.amazon.smithy.rust.codegen.client.smithy.customize.RequiredCustomizations
import software.amazon.smithy.rust.codegen.client.smithy.customize.RustCodegenDecorator
import software.amazon.smithy.rust.codegen.server.smithy.customizations.ServerRequiredCustomizations

internal class CombinedCodegenDecoratorTest {
    private val clientDecorator: RustCodegenDecorator<ClientCodegenContext> = RequiredCustomizations()
    private val serverDecorator: RustCodegenDecorator<ServerCodegenContext> = ServerRequiredCustomizations()

    @Test
    fun filterClientDecorators() {
        val filteredDecorators = CombinedCodegenDecorator.filterDecorators<ClientCodegenContext>(
            listOf(clientDecorator, serverDecorator),
        ).toList()

        filteredDecorators.shouldContainExactly(clientDecorator)
    }

    @Test
    fun filterServerDecorators() {
        val filteredDecorators = CombinedCodegenDecorator.filterDecorators<ServerCodegenContext>(
            listOf(clientDecorator, serverDecorator),
        ).toList()

        filteredDecorators.shouldContainExactly(serverDecorator)
    }
}
