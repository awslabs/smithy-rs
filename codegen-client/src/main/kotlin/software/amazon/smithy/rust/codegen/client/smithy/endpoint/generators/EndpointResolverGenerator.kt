/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

package software.amazon.smithy.rust.codegen.client.smithy.endpoint.generators

import software.amazon.smithy.rulesengine.language.Endpoint
import software.amazon.smithy.rulesengine.language.EndpointRuleSet
import software.amazon.smithy.rulesengine.language.eval.Type
import software.amazon.smithy.rulesengine.language.syntax.expr.Expression
import software.amazon.smithy.rulesengine.language.syntax.expr.Reference
import software.amazon.smithy.rulesengine.language.syntax.fn.Function
import software.amazon.smithy.rulesengine.language.syntax.fn.IsSet
import software.amazon.smithy.rulesengine.language.syntax.rule.Condition
import software.amazon.smithy.rulesengine.language.syntax.rule.Rule
import software.amazon.smithy.rulesengine.language.visit.RuleValueVisitor
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.Context
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.Types
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.endpointsLib
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.memberName
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.rulesgen.ExpressionGenerator
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.rulesgen.Ownership
import software.amazon.smithy.rust.codegen.client.smithy.endpoint.rustName
import software.amazon.smithy.rust.codegen.core.rustlang.Attribute
import software.amazon.smithy.rust.codegen.core.rustlang.RustWriter
import software.amazon.smithy.rust.codegen.core.rustlang.Writable
import software.amazon.smithy.rust.codegen.core.rustlang.comment
import software.amazon.smithy.rust.codegen.core.rustlang.escape
import software.amazon.smithy.rust.codegen.core.rustlang.join
import software.amazon.smithy.rust.codegen.core.rustlang.rust
import software.amazon.smithy.rust.codegen.core.rustlang.rustTemplate
import software.amazon.smithy.rust.codegen.core.rustlang.toType
import software.amazon.smithy.rust.codegen.core.rustlang.writable
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeConfig
import software.amazon.smithy.rust.codegen.core.smithy.RuntimeType
import software.amazon.smithy.rust.codegen.core.util.dq
import software.amazon.smithy.rust.codegen.core.util.orNull

abstract class CustomRuntimeFunction {
    abstract val id: String

    /** Initialize the struct field to a default value */
    abstract fun structFieldInit(): Writable?

    /** The argument slot of the runtime function. MUST NOT end with `,`
     * e.g `partition_data: &PartitionData`
     * */
    abstract fun additionalArgsSignature(): Writable?

    /**
     * A writable that passes additional args from `self` into the function.
     *
     * - Must match the order of additionalArgsSignature
     * - Must not end with `,`
     */
    abstract fun additionalArgsInvocation(self: String): Writable?

    /**
     * Any additional struct fields this runtime function adds to the resolver
     */
    abstract fun structField(): Writable?

    /**
     * Invoking the runtime function—(parens / args not needed): `$fn`
     *
     * e.g. `crate::endpoint_lib::uri_encode::uri_encode`
     *
     * The function signature must match the standard endpoints function signature:
     * - arguments in the order matching the spec
     * - additionalArgsInvocation (if needed)
     * - &mut DiagnosticCollector
     */
    abstract fun usage(): Writable
}

class FunctionRegistry(private val functions: List<CustomRuntimeFunction>) {
    private var usedFunctions = mutableSetOf<CustomRuntimeFunction>()
    fun fnFor(id: String): CustomRuntimeFunction? =
        functions.firstOrNull { it.id == id }?.also { usedFunctions.add(it) }

    fun fnsUsed(): List<CustomRuntimeFunction> = usedFunctions.toList().sortedBy { it.id }
}

/**
 * Generate an endpoint resolver struct. The struct may contain additional fields required by the usage of
 * additional functions e.g. `aws.partition` requires a `PartitionResolver` to cache the parsed result of `partitions.json`
 * and to facilitate loading additional partitions at runtime.
 *
 * Additionally, runtime functions will conditionally bring in:
 * 1. resolver configuration (e.g. a custom partitions.json)
 * 2. extra function arguments in the resolver
 * 3. the runtime type of the library function
 *
 * These dependencies are only brought in when the rules actually use these functions.
 *
 * ```rust
 * pub struct DefaultResolver {
 *   partition: PartitionResolver
 * }
 *
 * impl aws_smithy_http::endpoint::ResolveEndpoint<crate::endpoint::Params> for DefaultResolver {
 *     fn resolve_endpoint(&self, params: &Params) -> aws_smithy_http::endpoint::Result {
 *         let mut diagnostic_collector = crate::endpoint_lib::diagnostic::DiagnosticCollector::new();
 *         crate::endpoint::internals::resolve_endpoint(params, &self.partition_resolver, &mut diagnostic_collector)
 *             .map_err(|err| err.with_source(diagnostic_collector.take_last_error()))
 *     }
 * }
 *
 * mod internals {
 *   fn resolve_endpoint(params: &crate::endpoint::Params,
 *      // conditionally inserted, only when used
 *      partition_resolver: &PartitionResolver, _diagnostics: &mut DiagnosticCollector) -> endpoint::Result {
 *      // lots of generated code to actually resolve an endpoint
 *   }
 * }
 * ```
 *
 */

internal class EndpointResolverGenerator(stdlib: List<CustomRuntimeFunction>, runtimeConfig: RuntimeConfig) {
    private val registry: FunctionRegistry = FunctionRegistry(stdlib)
    private val types = Types(runtimeConfig)
    private val codegenScope = arrayOf(
        "endpoint" to types.smithyHttpEndpointModule,
        "SmithyEndpoint" to types.smithyEndpoint,
        "EndpointError" to types.resolveEndpointError,
        "DiagnosticCollector" to endpointsLib("diagnostic").toType().resolve("DiagnosticCollector"),
    )

    private val allowLintsForResolver = listOf(
        // we generate if x { if y { if z { ... } } }
        "clippy::collapsible_if",
        // we generate `if (true) == expr { ... }`
        "clippy::bool_comparison",
        // we generate `if !(a == b)`
        "clippy::nonminimal_bool",
        // we generate `if x == "" { ... }`
        "clippy::comparison_to_empty",
        // we generate `if let Some(_) = ... { ... }`
        "clippy::redundant_pattern_matching",
    )
    private val context = Context(registry, runtimeConfig)

    companion object {
        const val DiagnosticCollector = "_diagnostic_collector"
        private const val ParamsName = "_params"
    }

    /**
     * Generates the endpoint resolver struct
     *
     * If the rules require a runtime function that has state (e.g., the partition resolver,
     * the `[CustomRuntimeFunction.structField]`) will insert the required fields into the resolver so that they can
     * be used later.
     */
    fun defaultEndpointResolver(endpointRuleSet: EndpointRuleSet): RuntimeType {
        check(endpointRuleSet.rules.isNotEmpty()) { "EndpointRuleset must contain at least one rule." }
        // Here, we play a little trick: we run the resolver and actually render it into a writer. This allows the
        // function registry to record what functions we actually need. We need to do this, because the functions we
        // actually use impacts the function signature that we need to return
        resolverFnBody(endpointRuleSet)(RustWriter.root())

        // Now that we rendered the rules once (and then threw it away) we can see what functions we actually used!
        val fnsUsed = registry.fnsUsed()
        return RuntimeType.forInlineFun("DefaultResolver", EndpointsModule) {
            rustTemplate(
                """
                /// The default endpoint resolver
                ##[derive(Default)]
                pub struct DefaultResolver {
                    #{custom_fields:W}
                }

                impl DefaultResolver {
                    /// Create a new endpoint resolver with default settings
                    pub fn new() -> Self {
                        Self { #{custom_fields_init:W} }
                    }
                }

                impl #{endpoint}::ResolveEndpoint<#{Params}> for DefaultResolver {
                    fn resolve_endpoint(&self, params: &Params) -> #{endpoint}::Result {
                        let mut diagnostic_collector = #{DiagnosticCollector}::new();
                        #{resolver_fn}(params, &mut diagnostic_collector, #{additional_args})
                            .map_err(|err|err.with_source(diagnostic_collector.take_last_error()))
                    }
                }
                """,
                "custom_fields" to fnsUsed.mapNotNull { it.structField() }.join(","),
                "custom_fields_init" to fnsUsed.mapNotNull { it.structFieldInit() }.join(","),
                "Params" to EndpointParamsGenerator(endpointRuleSet.parameters).paramsStruct(),
                "additional_args" to fnsUsed.mapNotNull { it.additionalArgsInvocation("self") }.join(","),
                "resolver_fn" to resolverFn(endpointRuleSet, fnsUsed),
                *codegenScope,
            )
        }
    }

    private fun resolverFn(
        endpointRuleSet: EndpointRuleSet,
        fnsUsed: List<CustomRuntimeFunction>,
    ): RuntimeType {
        return RuntimeType.forInlineFun("resolve_endpoint", EndpointsImpl) {
            allowLintsForResolver.map { Attribute.Custom("allow($it)") }.map { it.render(this) }
            rustTemplate(
                """
                pub(super) fn resolve_endpoint($ParamsName: &#{Params}, $DiagnosticCollector: &mut #{DiagnosticCollector}, #{additional_args}) -> #{endpoint}::Result {
                  #{body:W}
                }

                """,
                *codegenScope,
                "Params" to EndpointParamsGenerator(endpointRuleSet.parameters).paramsStruct(),
                "additional_args" to fnsUsed.mapNotNull { it.additionalArgsSignature() }.join(","),
                "body" to resolverFnBody(endpointRuleSet),
            )
        }
    }

    private fun resolverFnBody(endpointRuleSet: EndpointRuleSet) = writable {
        endpointRuleSet.parameters.toList().forEach {
            Attribute.AllowUnused.render(this)
            rust("let ${it.memberName()} = &$ParamsName.${it.memberName()};")
        }
        generateRulesList(endpointRuleSet.rules)(this)
    }

    private fun generateRulesList(rules: List<Rule>) = writable {
        rules.forEach { rule ->
            rule.documentation.orNull()?.also { comment(escape(it)) }
            generateRule(rule)(this)
        }
        if (!isExhaustive(rules.last())) {
            // it's hard to figure out if these are always needed or not
            Attribute.Custom("allow(unreachable_code)").render(this)
            rustTemplate(
                """return Err(#{EndpointError}::message(format!("No rules matched these parameters. This is a bug. {:?}", $ParamsName)));""",
                *codegenScope,
            )
        }
    }

    private fun isExhaustive(rule: Rule): Boolean = rule.conditions.isEmpty() || rule.conditions.all {
        when (it.fn.type()) {
            is Type.Bool -> false
            is Type.Option -> false
            else -> true
        }
    }

    private fun generateRule(rule: Rule): Writable {
        return generateRuleInternal(rule, rule.conditions)
    }

    /**
     * deal with the actual target of the condition but flattening through isSet
     */
    private fun Condition.targetFunction(): Expression {
        return when (val fn = this.fn) {
            is IsSet -> fn.target
            else -> fn
        }
    }

    /**
     * Recursive function is generate a rule and its list of conditions.
     *
     * The resulting generated code is a series of nested-if statements, nesting each condition inside the previous.
     */
    private fun generateRuleInternal(rule: Rule, conditions: List<Condition>): Writable {
        if (conditions.isEmpty()) {
            return rule.accept(RuleVisitor())
        } else {
            val condition = conditions.first()
            val rest = conditions.drop(1)
            return {
                val generator = ExpressionGenerator(Ownership.Borrowed, context)
                val fn = condition.targetFunction()

                // there are three patterns we need to handle:
                // 1. the RHS returns an option which we need to guard as "Some(...)"
                // 2. the RHS returns a boolean which we need to gate on
                // 3. the RHS is infallible (e.g. uriEncode)
                val resultName =
                    (condition.result.orNull() ?: (fn as? Reference)?.name)?.rustName() ?: "_"
                val target = generator.generate(fn)
                val next = generateRuleInternal(rule, rest)
                when {
                    fn.type() is Type.Option ||
                        // TODO(https://github.com/awslabs/smithy/pull/1504): ReterminusCore bug: substring should return `Option<String>`:
                        (fn as Function).name == "substring" -> {
                        Attribute.AllowUnused.render(this)
                        rustTemplate(
                            "if let Some($resultName) = #{target:W} { #{next:W} }",
                            "target" to target,
                            "next" to next,
                        )
                    }

                    fn.type() is Type.Bool -> {
                        rustTemplate(
                            """
                            if #{target:W} {#{binding}
                                #{next:W}
                            }
                            """,
                            "target" to target,
                            "next" to next,
                            // handle the rare but possible case where we bound the name of a variable to a boolean condition
                            "binding" to writable {
                                if (resultName != "_") {
                                    rust("let $resultName = true;")
                                }
                            },
                        )
                    }

                    else -> {
                        // the function is infallible: just create a binding
                        rustTemplate(
                            """
                            let $resultName = #{target:W};
                            #{next:W}
                            """,
                            "target" to generator.generate(fn),
                            "next" to generateRuleInternal(rule, rest),
                        )
                    }
                }
            }
        }
    }

    inner class RuleVisitor : RuleValueVisitor<Writable> {
        override fun visitTreeRule(rules: List<Rule>) = generateRulesList(rules)

        override fun visitErrorRule(error: Expression) = writable {
            rustTemplate(
                "return Err(#{EndpointError}::message(#{message:W}));",
                *codegenScope,
                "message" to ExpressionGenerator(Ownership.Owned, context).generate(error),
            )
        }

        override fun visitEndpointRule(endpoint: Endpoint): Writable = writable {
            rust("return Ok(#W);", generateEndpoint(endpoint))
        }
    }

    /**
     * generate the rust code for `[endpoint]`
     */
    internal fun generateEndpoint(endpoint: Endpoint): Writable {
        val generator = ExpressionGenerator(Ownership.Owned, context)
        val url = generator.generate(endpoint.url)
        val headers = endpoint.headers.mapValues { entry -> entry.value.map { generator.generate(it) } }
        val properties = endpoint.properties.mapValues { entry -> generator.generate(entry.value) }
        return writable {
            rustTemplate("#{SmithyEndpoint}::builder().url(#{url:W})", *codegenScope, "url" to url)
            headers.forEach { (name, values) -> values.forEach { rust(".header(${name.dq()}, #W)", it) } }
            properties.forEach { (name, value) -> rust(".property(${name.asString().dq()}, #W)", value) }
            rust(".build()")
        }
    }
}
