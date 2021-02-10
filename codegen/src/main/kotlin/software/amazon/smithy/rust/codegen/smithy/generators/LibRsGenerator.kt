/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import software.amazon.smithy.rust.codegen.rustlang.RustModule
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.rustlang.escape

class LibRsGenerator(private val libraryDocs: String, private val modules: List<RustModule>) {
    fun render(writer: RustWriter) {
        writer.setHeaderDocs(writer.escape(libraryDocs))
        modules.forEach { it.render(writer) }
    }
}
