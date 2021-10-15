/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

package software.amazon.smithy.rust.codegen.smithy

import software.amazon.smithy.build.FileManifest
import software.amazon.smithy.codegen.core.SymbolProvider
import software.amazon.smithy.codegen.core.writer.CodegenWriterDelegator
import software.amazon.smithy.model.Model
import software.amazon.smithy.model.shapes.Shape
import software.amazon.smithy.rust.codegen.rustlang.CargoDependency
import software.amazon.smithy.rust.codegen.rustlang.Feature
import software.amazon.smithy.rust.codegen.rustlang.InlineDependency
import software.amazon.smithy.rust.codegen.rustlang.RustDependency
import software.amazon.smithy.rust.codegen.rustlang.RustModule
import software.amazon.smithy.rust.codegen.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.smithy.generators.CargoTomlGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.LibRsCustomization
import software.amazon.smithy.rust.codegen.smithy.generators.LibRsGenerator
import software.amazon.smithy.rust.codegen.smithy.generators.ManifestCustomizations

/**
 * RustCrate abstraction.
 *
 * **Note**: This is the only implementation, `open` only for test purposes.
 *
 * All code-generation at some point goes through this class. `RustCrate` maintains a `CodegenWriterDelegator` internally
 * which tracks a set of file-writer pairs and allows them to be loaded and cached (see: [useShapeWriter])
 *
 * On top of this, it adds Rust specific features:
 * - Generation of a `lib.rs` which adds `mod` statements automatically for every module that was used
 * - Tracking dependencies and crate features used during code generation, enabling generation of `Cargo.toml`
 *
 * Users will generally want to use two main entry points:
 * 1. [useShapeWriter]: Find or create a writer that will contain a given shape. See [locatedIn] for context about how
 *    shape locations are determined.
 * 2. [finalize]: Write the crate out to the file system, generating a lib.rs and Cargo.toml
 */
open class RustCrate(
    fileManifest: FileManifest,
    symbolProvider: SymbolProvider,
    /**
     * For core modules like `input`, `output`, and `error`, we need to specify whether these modules should be public or
     * private as well as any other metadata. [baseModules] enables configuring this. See [DefaultPublicModules].
     */
    baseModules: Map<String, RustModule>
) {
    private val inner = CodegenWriterDelegator(fileManifest, symbolProvider, RustWriter.Factory)
    private val modules: MutableMap<String, RustModule> = baseModules.toMutableMap()
    private val features: MutableSet<Feature> = mutableSetOf()

    /**
     * Write into the module that this shape is [locatedIn]
     */
    fun useShapeWriter(shape: Shape, f: (RustWriter) -> Unit) {
        inner.useShapeWriter(shape, f)
    }

    /**
     * Write directly into lib.rs
     */
    fun lib(moduleWriter: (RustWriter) -> Unit) {
        inner.useFileWriter("src/lib.rs", "crate", moduleWriter)
    }

    fun mergeFeature(feature: Feature) {
        when (val existing = features.firstOrNull { it.name == feature.name }) {
            null -> features.add(feature)
            else -> {
                features.remove(existing)
                features.add(
                    existing.copy(
                        deps = (existing.deps + feature.deps).toSortedSet().toList()
                    )
                )
            }
        }
    }

    /**
     * Finalize Cargo.toml and lib.rs and flush the writers to the file system.
     *
     * This is also where inline dependencies are actually reified and written, potentially recursively.
     */
    fun finalize(
        settings: RustSettings,
        model: Model,
        manifestCustomizations: ManifestCustomizations,
        libRsCustomizations: List<LibRsCustomization>
    ) {
        injectInlineDependencies()
        val modules = inner.writers.values.mapNotNull { it.module() }.filter { it != "lib" }
            .map { modules[it] ?: RustModule.default(it, false) }
        inner.finalize(settings, model, manifestCustomizations, libRsCustomizations, modules, this.features.toList())
    }

    private fun injectInlineDependencies() {
        val writtenDependencies = mutableSetOf<String>()
        val unloadedDependencies = {
            this
                .inner.dependencies
                .map { dep -> RustDependency.fromSymbolDependency(dep) }
                .filterIsInstance<InlineDependency>().distinctBy { it.key() }
                .filter { !writtenDependencies.contains(it.key()) }
        }
        while (unloadedDependencies().isNotEmpty()) {
            unloadedDependencies().forEach { dep ->
                writtenDependencies.add(dep.key())
                this.withModule(dep.module) {
                    dep.renderer(it)
                }
            }
        }
    }

    /**
     * Create a new module directly. The resulting module will be placed in `src/<modulename>.rs`
     */
    fun withModule(
        module: RustModule,
        moduleWriter: (RustWriter) -> Unit
    ): RustCrate {
        val moduleName = module.name
        modules[moduleName] = module
        inner.useFileWriter("src/$moduleName.rs", "crate::$moduleName", moduleWriter)
        return this
    }

    /**
     * Create a new file directly
     */
    fun withFile(filename: String, fileWriter: (RustWriter) -> Unit) {
        inner.useFileWriter(filename) {
            fileWriter(it)
        }
    }
}

// TODO: this should _probably_ be configurable via RustSettings; 2h
/**
 * Allowlist of modules that will be exposed publicly in generated crates
 */
val DefaultPublicModules =
    setOf("error", "operation", "model", "input", "output", "config").map { it to RustModule.default(it, true) }.toMap()

/**
 * Finalize all the writers by:
 * - inlining inline dependencies that have been used
 * - generating (and writing) a Cargo.toml based on the settings & the required dependencies
 */
fun CodegenWriterDelegator<RustWriter>.finalize(
    settings: RustSettings,
    model: Model,
    manifestCustomizations: ManifestCustomizations,
    libRsCustomizations: List<LibRsCustomization>,
    modules: List<RustModule>,
    features: List<Feature>
) {
    this.useFileWriter("src/lib.rs", "crate::lib") { writer ->
        LibRsGenerator(settings, model, modules, libRsCustomizations).render(writer)
    }
    val cargoDependencies = mergeDependencyFeatures(
        this.dependencies.map { RustDependency.fromSymbolDependency(it) }
            .filterIsInstance<CargoDependency>().distinct()
    )
    this.useFileWriter("Cargo.toml") {
        val cargoToml = CargoTomlGenerator(
            settings,
            it,
            manifestCustomizations,
            cargoDependencies,
            features
        )
        cargoToml.render()
    }
    flushWriters()
}

private fun CargoDependency.mergeWith(other: CargoDependency): CargoDependency {
    check(key == other.key)
    return copy(
        features = features + other.features,
        optional = optional && other.optional
    )
}

internal fun mergeDependencyFeatures(cargoDependencies: List<CargoDependency>): List<CargoDependency> =
    cargoDependencies.groupBy { it.key }
        .mapValues { group -> group.value.reduce { acc, next -> acc.mergeWith(next) } }
        .values
        .toList()
        .sortedBy { it.name }
