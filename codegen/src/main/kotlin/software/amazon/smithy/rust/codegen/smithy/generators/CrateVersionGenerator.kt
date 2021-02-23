/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy.generators

import software.amazon.smithy.rust.codegen.rustlang.rust
import software.amazon.smithy.rust.codegen.rustlang.writable

/**
 * Add `PGK_VERSION` const in lib.rs to enable knowing the version of the current module
 */
class CrateVersionGenerator : LibRsCustomization() {
    override fun section(section: LibRsSection) = writable {
        when (section) {
            is LibRsSection.Body -> rust("""pub static PKG_VERSION: &str = env!("CARGO_PKG_VERSION");""")
        }
    }
}
