$version: "2.0"

namespace smithy.protocoltests.rpcv2Cbor

use smithy.framework#ValidationException
use smithy.protocols#rpcv2Cbor
use smithy.test#httpResponseTests
use smithy.test#httpMalformedRequestTests

@rpcv2Cbor
service RpcV2Service {
    operations: [
        SimpleStructOperation
        ErrorSerializationOperation
        ComplexStructOperation
        EmptyStructOperation
        SingleMemberStructOperation
    ]
}

// TODO(https://github.com/smithy-lang/smithy/issues/2326): Smithy should not
// allow HTTP binding traits in this protocol.
@http(uri: "/simple-struct-operation", method: "POST")
operation SimpleStructOperation {
    input: SimpleStruct
    output: SimpleStruct
    errors: [ValidationException]
}

operation ErrorSerializationOperation {
    input: SimpleStruct
    output: ErrorSerializationOperationOutput
    errors: [ValidationException]
}

operation ComplexStructOperation {
    input: ComplexStruct
    output: ComplexStruct
    errors: [ValidationException]
}

operation EmptyStructOperation {
    input: EmptyStruct
    output: EmptyStruct
}

operation SingleMemberStructOperation {
    input: SingleMemberStruct
    output: SingleMemberStruct
}

apply EmptyStructOperation @httpMalformedRequestTests([
    {
        id: "AdditionalTokensEmptyStruct",
        documentation: """
        When additional tokens are found past where we expect the end of the body,
        the request should be rejected with a serialization exception.""",
        protocol: rpcv2Cbor,
        request: {
            method: "POST",
            uri: "/service/RpcV2Service/operation/EmptyStructOperation",
            headers: {
                "smithy-protocol": "rpc-v2-cbor",
                "Accept": "application/cbor",
                "Content-Type": "application/cbor"
            }
            // Two empty variable-length encoded CBOR maps back to back.
            body: "v/+//w=="
        },
        response: {
            code: 400,
            body: {
                mediaType: "application/cbor",
                assertion: {
                    // An empty CBOR map.
                    // TODO(https://github.com/smithy-lang/smithy-rs/issues/3716): we're not serializing `__type` because `SerializationException` is not modeled.
                    contents: "oA=="
                }
            }
        }

    }
])

apply SingleMemberStructOperation @httpMalformedRequestTests([
    {
        id: "AdditionalTokensSingleMemberStruct",
        documentation: """
        When additional tokens are found past where we expect the end of the body,
        the request should be rejected with a serialization exception.""",
        protocol: rpcv2Cbor,
        request: {
            method: "POST",
            uri: "/service/RpcV2Service/operation/SingleMemberStructOperation",
            headers: {
                "smithy-protocol": "rpc-v2-cbor",
                "Accept": "application/cbor",
                "Content-Type": "application/cbor"
            }
            // Two empty variable-length encoded CBOR maps back to back.
            body: "v/+//w=="
        },
        response: {
            code: 400,
            body: {
                mediaType: "application/cbor",
                assertion: {
                    // An empty CBOR map.
                    // TODO(https://github.com/smithy-lang/smithy-rs/issues/3716): we're not serializing `__type` because `SerializationException` is not modeled.
                    contents: "oA=="
                }
            }
        }
    }
])

apply ErrorSerializationOperation @httpMalformedRequestTests([
    {
        id: "ErrorSerializationIncludesTypeField",
        documentation: """
        When invalid input is provided the request should be rejected with
        a validation exception, and a `__type` field should be included""",
        protocol: rpcv2Cbor,
        request: {
            method: "POST",
            uri: "/service/RpcV2Service/operation/ErrorSerializationOperation",
            headers: {
                "smithy-protocol": "rpc-v2-cbor",
                "Accept": "application/cbor",
                "Content-Type": "application/cbor"
            }
            // An empty CBOR map. We're missing a lot of `@required` members!
            body: "oA=="
        },
        response: {
            code: 400,
            body: {
                mediaType: "application/cbor",
                assertion: {
                    contents: "v2ZfX3R5cGV4JHNtaXRoeS5mcmFtZXdvcmsjVmFsaWRhdGlvbkV4Y2VwdGlvbmdtZXNzYWdleGsxIHZhbGlkYXRpb24gZXJyb3IgZGV0ZWN0ZWQuIFZhbHVlIGF0ICcvcmVxdWlyZWRCbG9iJyBmYWlsZWQgdG8gc2F0aXNmeSBjb25zdHJhaW50OiBNZW1iZXIgbXVzdCBub3QgYmUgbnVsbGlmaWVsZExpc3SBv2RwYXRobS9yZXF1aXJlZEJsb2JnbWVzc2FnZXhOVmFsdWUgYXQgJy9yZXF1aXJlZEJsb2InIGZhaWxlZCB0byBzYXRpc2Z5IGNvbnN0cmFpbnQ6IE1lbWJlciBtdXN0IG5vdCBiZSBudWxs//8="
                }
            }
        }
    }
])

apply ErrorSerializationOperation @httpResponseTests([
    {
        id: "OperationOutputSerializationQuestionablyIncludesTypeField",
        documentation: """
        Despite the operation output being a structure shape with the `@error` trait,
        `__type` field should, in a strict interpretation of the spec, not be included, 
        because we're not serializing a server error response. However, we do, because 
        there shouldn't™️ be any harm in doing so, and it greatly simplifies the 
        code generator. This test just pins this behavior in case we ever modify it.""",
        protocol: rpcv2Cbor,
        code: 200,
        params: {
            errorShape: {
                message: "ValidationException message field"
            }
        }
        bodyMediaType: "application/cbor"
        body: "v2plcnJvclNoYXBlv2ZfX3R5cGV4JHNtaXRoeS5mcmFtZXdvcmsjVmFsaWRhdGlvbkV4Y2VwdGlvbmdtZXNzYWdleCFWYWxpZGF0aW9uRXhjZXB0aW9uIG1lc3NhZ2UgZmllbGT//w=="
    }
])

apply SimpleStructOperation @httpResponseTests([
    {
        id: "SimpleStruct",
        protocol: rpcv2Cbor,
        code: 200, // Not used.
        params: {
            blob: "blobby blob",
            boolean: false,

            string: "There are three things all wise men fear: the sea in storm, a night with no moon, and the anger of a gentle man.",

            byte: 69,
            short: 70,
            integer: 71,
            long: 72,

            float: 0.69,
            double: 0.6969,

            timestamp: 1546300800,
            enum: "DIAMOND"

            // With `@required`.

            requiredBlob: "blobby blob",
            requiredBoolean: false,

            requiredString: "There are three things all wise men fear: the sea in storm, a night with no moon, and the anger of a gentle man.",

            requiredByte: 69,
            requiredShort: 70,
            requiredInteger: 71,
            requiredLong: 72,

            requiredFloat: 0.69,
            requiredDouble: 0.6969,

            requiredTimestamp: 1546300800,
            requiredEnum: "DIAMOND"
        }
    },
    // Same test, but leave optional types empty
    {
        id: "SimpleStructWithOptionsSetToNone",
        protocol: rpcv2Cbor,
        code: 200, // Not used.
        params: {
            requiredBlob: "blobby blob",
            requiredBoolean: false,

            requiredString: "There are three things all wise men fear: the sea in storm, a night with no moon, and the anger of a gentle man.",

            requiredByte: 69,
            requiredShort: 70,
            requiredInteger: 71,
            requiredLong: 72,

            requiredFloat: 0.69,
            requiredDouble: 0.6969,

            requiredTimestamp: 1546300800,
            requiredEnum: "DIAMOND"
        }
    }
])

structure ErrorSerializationOperationOutput {
    errorShape: ValidationException
}

structure SimpleStruct {
    blob: Blob
    boolean: Boolean

    string: String

    byte: Byte
    short: Short
    integer: Integer
    long: Long

    float: Float
    double: Double

    timestamp: Timestamp
    enum: Suit

    // With `@required`.

    @required requiredBlob: Blob
    @required requiredBoolean: Boolean

    @required requiredString: String

    @required requiredByte: Byte
    @required requiredShort: Short
    @required requiredInteger: Integer
    @required requiredLong: Long

    @required requiredFloat: Float
    @required requiredDouble: Double

    @required requiredTimestamp: Timestamp
    // @required requiredDocument: MyDocument
    @required requiredEnum: Suit
}

structure ComplexStruct {
    structure: SimpleStruct
    emptyStructure: EmptyStruct
    list: SimpleList
    map: SimpleMap
    union: SimpleUnion
    unitUnion: UnitUnion

    structureList: StructList

    // `@required` for good measure here.
    @required complexList: ComplexList
    @required complexMap: ComplexMap
    @required complexUnion: ComplexUnion
}

structure EmptyStruct { }

structure SingleMemberStruct {
    message: String
}

list StructList {
    member: SimpleStruct
}

list SimpleList {
    member: String
}

map SimpleMap {
    key: String
    value: Integer
}

// TODO(https://github.com/smithy-lang/smithy/issues/2325): Upstream protocol
// test suite doesn't cover unions. While the generated SDK compiles, we're not
// exercising the (de)serializers with actual values.
union SimpleUnion {
    blob: Blob
    boolean: Boolean
    string: String
    unit: Unit
}

union UnitUnion {
    unitA: Unit
    unitB: Unit
}

list ComplexList {
    member: ComplexMap
}

map ComplexMap {
    key: String
    value: ComplexUnion
}

union ComplexUnion {
    // Recursive path here.
    complexStruct: ComplexStruct

    structure: SimpleStruct
    list: SimpleList
    map: SimpleMap
    union: SimpleUnion
}

enum Suit {
    DIAMOND
    CLUB
    HEART
    SPADE
}
