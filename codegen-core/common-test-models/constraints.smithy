$version: "1.0"

namespace com.amazonaws.constraints

use aws.protocols#restJson1
use smithy.framework#ValidationException

/// A service to test aspects of code generation where shapes have constraint traits.
@restJson1
@title("ConstraintsService")
service ConstraintsService {
    operations: [
        // TODO Rename as {Verb}[{Qualifier}]{Noun}: https://github.com/awslabs/smithy-rs/pull/1342#discussion_r980936650
        ConstrainedShapesOperation,
        ConstrainedHttpBoundShapesOperation,
        ConstrainedRecursiveShapesOperation,
        // `httpQueryParams` and `httpPrefixHeaders` are structurually
        // exclusive, so we need one operation per target shape type
        // combination.
        QueryParamsTargetingLengthMapOperation,
        QueryParamsTargetingMapOfLengthStringOperation,
        QueryParamsTargetingMapOfListOfLengthStringOperation,
        QueryParamsTargetingMapOfSetOfLengthStringOperation,
        HttpPrefixHeadersTargetingLengthMapOperation,

        QueryParamsTargetingMapOfRangeIntegerOperation,
        QueryParamsTargetingMapOfListOfRangeIntegerOperation,
        QueryParamsTargetingMapOfSetOfRangeIntegerOperation,

        QueryParamsTargetingMapOfEnumStringOperation,
        QueryParamsTargetingMapOfListOfEnumStringOperation,
        // TODO(https://github.com/awslabs/smithy-rs/issues/1431)
        // HttpPrefixHeadersTargetingMapOfEnumStringOperation,

        NonStreamingBlobOperation,

        StreamingBlobOperation,
        EventStreamsOperation,
    ],
}

@http(uri: "/constrained-shapes-operation", method: "POST")
operation ConstrainedShapesOperation {
    input: ConstrainedShapesOperationInputOutput,
    output: ConstrainedShapesOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/constrained-http-bound-shapes-operation/{lengthStringLabel}/{enumStringLabel}", method: "POST")
operation ConstrainedHttpBoundShapesOperation {
    input: ConstrainedHttpBoundShapesOperationInputOutput,
    output: ConstrainedHttpBoundShapesOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/constrained-recursive-shapes-operation", method: "POST")
operation ConstrainedRecursiveShapesOperation {
    input: ConstrainedRecursiveShapesOperationInputOutput,
    output: ConstrainedRecursiveShapesOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-length-map", method: "POST")
operation QueryParamsTargetingLengthMapOperation {
    input: QueryParamsTargetingLengthMapOperationInputOutput,
    output: QueryParamsTargetingLengthMapOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-length-string-operation", method: "POST")
operation QueryParamsTargetingMapOfLengthStringOperation {
    input: QueryParamsTargetingMapOfLengthStringOperationInputOutput,
    output: QueryParamsTargetingMapOfLengthStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-range-integer-operation", method: "POST")
operation QueryParamsTargetingMapOfRangeIntegerOperation {
    input: QueryParamsTargetingMapOfRangeIntegerOperationInputOutput,
    output: QueryParamsTargetingMapOfRangeIntegerOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-enum-string-operation", method: "POST")
operation QueryParamsTargetingMapOfEnumStringOperation {
    input: QueryParamsTargetingMapOfEnumStringOperationInputOutput,
    output: QueryParamsTargetingMapOfEnumStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-list-of-length-string-operation", method: "POST")
operation QueryParamsTargetingMapOfListOfLengthStringOperation {
    input: QueryParamsTargetingMapOfListOfLengthStringOperationInputOutput,
    output: QueryParamsTargetingMapOfListOfLengthStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-list-of-range-integer-operation", method: "POST")
operation QueryParamsTargetingMapOfListOfRangeIntegerOperation {
    input: QueryParamsTargetingMapOfListOfRangeIntegerOperationInputOutput,
    output: QueryParamsTargetingMapOfListOfRangeIntegerOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-set-of-length-string-operation", method: "POST")
operation QueryParamsTargetingMapOfSetOfLengthStringOperation {
    input: QueryParamsTargetingMapOfSetOfLengthStringOperationInputOutput,
    output: QueryParamsTargetingMapOfSetOfLengthStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-set-of-range-integer-operation", method: "POST")
operation QueryParamsTargetingMapOfSetOfRangeIntegerOperation {
    input: QueryParamsTargetingMapOfSetOfRangeIntegerOperationInputOutput,
    output: QueryParamsTargetingMapOfSetOfRangeIntegerOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-list-of-enum-string-operation", method: "POST")
operation QueryParamsTargetingMapOfListOfEnumStringOperation {
    input: QueryParamsTargetingMapOfListOfEnumStringOperationInputOutput,
    output: QueryParamsTargetingMapOfListOfEnumStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/http-prefix-headers-targeting-length-map-operation", method: "POST")
operation HttpPrefixHeadersTargetingLengthMapOperation {
    input: HttpPrefixHeadersTargetingLengthMapOperationInputOutput,
    output: HttpPrefixHeadersTargetingLengthMapOperationInputOutput,
    errors: [ValidationException],
}

@http(uri: "/http-prefix-headers-targeting-map-of-enum-string-operation", method: "POST")
operation HttpPrefixHeadersTargetingMapOfEnumStringOperation {
    input: HttpPrefixHeadersTargetingMapOfEnumStringOperationInputOutput,
    output: HttpPrefixHeadersTargetingMapOfEnumStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/non-streaming-blob-operation", method: "POST")
operation NonStreamingBlobOperation {
    input: NonStreamingBlobOperationInputOutput,
    output: NonStreamingBlobOperationInputOutput,
}

@http(uri: "/streaming-blob-operation", method: "POST")
operation StreamingBlobOperation {
    input: StreamingBlobOperationInputOutput,
    output: StreamingBlobOperationInputOutput,
}

@http(uri: "/event-streams-operation", method: "POST")
operation EventStreamsOperation {
    input: EventStreamsOperationInputOutput,
    output: EventStreamsOperationInputOutput,
}

structure ConstrainedShapesOperationInputOutput {
    @required
    conA: ConA,
}

structure ConstrainedHttpBoundShapesOperationInputOutput {
    @required
    @httpLabel
    lengthStringLabel: LengthString,

    @required
    @httpLabel
    rangeIntegerLabel: RangeInteger,

    @required
    @httpLabel
    enumStringLabel: EnumString,

    @required
    @httpPrefixHeaders("X-Length-String-Prefix-Headers-")
    lengthStringHeaderMap: MapOfLengthString,

    @required
    @httpPrefixHeaders("X-Range-Integer-Prefix-Headers-")
    rangeIntegerHeaderMap: MapOfRangeInteger,

    @httpHeader("X-Length")
    lengthStringHeader: LengthString,

    @httpHeader("X-Range-Integer")
    rangeIntegerHeader: RangeInteger,

    // @httpHeader("X-Length-MediaType")
    // lengthStringHeaderWithMediaType: MediaTypeLengthString,

    @httpHeader("X-Length-Set")
    lengthStringSetHeader: SetOfLengthString,

    @httpHeader("X-Length-List")
    lengthStringListHeader: ListOfLengthString,

    @httpHeader("X-Range-Integer-Set")
    rangeIntegerSetHeader: SetOfRangeInteger,

    @httpHeader("X-Range-Integer-List")
    rangeIntegerListHeader: ListOfRangeInteger,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1431)
    // @httpHeader("X-Enum")
    //enumStringHeader: EnumString,

    // @httpHeader("X-Enum-List")
    // enumStringListHeader: ListOfEnumString,

    @httpQuery("lengthString")
    lengthStringQuery: LengthString,

    @httpQuery("rangeInteger")
    rangeIntegerQuery: RangeInteger,

    @httpQuery("enumString")
    enumStringQuery: EnumString,

    @httpQuery("lengthStringList")
    lengthStringListQuery: ListOfLengthString,

    @httpQuery("lengthStringSet")
    lengthStringSetQuery: SetOfLengthString,

    @httpQuery("rangeIntegerList")
    rangeIntegerListQuery: ListOfRangeInteger,

    @httpQuery("rangeIntegerSet")
    rangeIntegerSetQuery: SetOfRangeInteger,

    @httpQuery("enumStringList")
    enumStringListQuery: ListOfEnumString,
}

structure HttpPrefixHeadersTargetingLengthMapOperationInputOutput {
    @httpPrefixHeaders("X-Prefix-Headers-LengthMap-")
    lengthMap: ConBMap,
}

structure HttpPrefixHeadersTargetingMapOfEnumStringOperationInputOutput {
    @httpPrefixHeaders("X-Prefix-Headers-MapOfEnumString-")
    mapOfEnumString: MapOfEnumString,
}

structure QueryParamsTargetingLengthMapOperationInputOutput {
    @httpQueryParams
    lengthMap: ConBMap
}

structure QueryParamsTargetingRangeIntegerMapOperationInputOutput {
    @httpQueryParams
    rangeIntegerMap: ConBMap
}

structure QueryParamsTargetingMapOfLengthStringOperationInputOutput {
    @httpQueryParams
    mapOfLengthString: MapOfLengthString
}

structure QueryParamsTargetingMapOfRangeIntegerOperationInputOutput {
    @httpQueryParams
    mapOfRangeInteger: MapOfRangedInteger
}

structure QueryParamsTargetingMapOfEnumStringOperationInputOutput {
    @httpQueryParams
    mapOfEnumString: MapOfEnumString
}

structure QueryParamsTargetingMapOfListOfLengthStringOperationInputOutput {
    @httpQueryParams
    mapOfListOfLengthString: MapOfListOfLengthString
}

structure QueryParamsTargetingMapOfListOfRangeIntegerOperationInputOutput {
    @httpQueryParams
    mapOfListOfRangeInteger: MapOfListOfRangeInteger
}

structure QueryParamsTargetingMapOfSetOfLengthStringOperationInputOutput {
    @httpQueryParams
    mapOfSetOfLengthString: MapOfSetOfLengthString
}

structure QueryParamsTargetingMapOfSetOfRangeIntegerOperationInputOutput {
    @httpQueryParams
    mapOfSetOfRangeInteger: MapOfSetOfRangeInteger
}

structure QueryParamsTargetingMapOfListOfEnumStringOperationInputOutput {
    @httpQueryParams
    mapOfListOfEnumString: MapOfListOfEnumString
}

structure NonStreamingBlobOperationInputOutput {
    @httpPayload
    nonStreamingBlob: NonStreamingBlob,
}

structure StreamingBlobOperationInputOutput {
    @httpPayload
    streamingBlob: StreamingBlob,
}

structure EventStreamsOperationInputOutput {
    @httpPayload
    events: Event,
}

@streaming
union Event {
    regularMessage: EventStreamRegularMessage,
    errorMessage: EventStreamErrorMessage,
}

structure EventStreamRegularMessage {
    messageContent: String
    // TODO(https://github.com/awslabs/smithy/issues/1388): Can't add a constraint trait here until the semantics are clarified.
    // messageContent: LengthString
}

@error("server")
structure EventStreamErrorMessage {
    messageContent: String
    // TODO(https://github.com/awslabs/smithy/issues/1388): Can't add a constraint trait here until the semantics are clarified.
    // messageContent: LengthString
}

// TODO(https://github.com/awslabs/smithy/issues/1389): Can't add a constraint trait here until the semantics are clarified.
@streaming
blob StreamingBlob

blob NonStreamingBlob

structure ConA {
    @required
    conB: ConB,

    optConB: ConB,

    lengthString: LengthString,
    minLengthString: MinLengthString,
    maxLengthString: MaxLengthString,
    fixedLengthString: FixedLengthString,

    rangeInteger: RangeInteger,
    minRangeInteger: MinRangeInteger,
    maxRangeInteger: MaxRangeInteger,
    fixedValueInteger: FixedValueInteger,

    conBList: ConBList,
    conBList2: ConBList2,

    conBSet: ConBSet,

    conBMap: ConBMap,

    mapOfMapOfListOfListOfConB: MapOfMapOfListOfListOfConB,

    constrainedUnion: ConstrainedUnion,
    enumString: EnumString,

    listOfLengthString: ListOfLengthString,
    setOfLengthString: SetOfLengthString,
    mapOfLengthString: MapOfLengthString,

    listOfRangeInteger: ListOfRangeInteger,
    setOfRangeInteger: SetOfRangeInteger,
    mapOfRangeInteger: MapOfRangeInteger,

    nonStreamingBlob: NonStreamingBlob
}

map MapOfLengthString {
    key: LengthString,
    value: LengthString,
}

map MapOfRangedInteger {
    key: RangeInteger,
    value: RangeInteger,
}

map MapOfEnumString {
    key: EnumString,
    value: EnumString,
}

map MapOfListOfLengthString {
    key: LengthString,
    value: ListOfLengthString,
}

map MapOfListOfRangeInteger {
    key: Rangedinteger,
    value: ListOfRangeInteger,
}

map MapOfListOfEnumString {
    key: EnumString,
    value: ListOfEnumString,
}

map MapOfSetOfLengthString {
    key: LengthString,
    value: SetOfLengthString,
}

map MapOfSetOfRangeInteger {
    key: RangeInteger,
    value: SetOfRangeInteger,
}

@length(min: 2, max: 8)
list LengthListOfLengthString {
    member: LengthString
}

@length(min: 2, max: 69)
string LengthString

@length(min: 2)
string MinLengthString

@length(max: 69)
string MaxLengthString

@length(min: 69, max: 69)
string FixedLengthString

@mediaType("video/quicktime")
@length(min: 1, max: 69)
string MediaTypeLengthString

@range(min: -0, max: 69)
integer RangeInteger

@range(min: -10)
integer MinRangeInteger

@range(max: 69)
integer MaxRangeInteger

@range(min: 69, max: 69)
integer FixedValueInteger

/// A union with constrained members.
union ConstrainedUnion {
    enumString: EnumString,
    lengthString: LengthString,

    constrainedStructure: ConB,
    conBList: ConBList,
    conBSet: ConBSet,
    conBMap: ConBMap,
}

@enum([
    {
        value: "t2.nano",
        name: "T2_NANO",
    },
    {
        value: "t2.micro",
        name: "T2_MICRO",
    },
    {
        value: "m256.mega",
        name: "M256_MEGA",
    }
])
string EnumString

set SetOfLengthString {
    member: LengthString
}

list ListOfLengthString {
    member: LengthString
}

set SetOfRangeInteger {
    member: RangeInteger
}

list ListOfRangeInteger {
    member: RangeInteger
}

list ListOfEnumString {
    member: EnumString
}

structure ConB {
    @required
    nice: String,
    @required
    int: Integer,

    optNice: String,
    optInt: Integer
}

structure ConstrainedRecursiveShapesOperationInputOutput {
    nested: RecursiveShapesInputOutputNested1,

    @required
    recursiveList: RecursiveList
}

structure RecursiveShapesInputOutputNested1 {
    @required
    recursiveMember: RecursiveShapesInputOutputNested2
}

structure RecursiveShapesInputOutputNested2 {
    recursiveMember: RecursiveShapesInputOutputNested1,
}

list RecursiveList {
    member: RecursiveShapesInputOutputNested1
}

list ConBList {
    member: NestedList
}

list ConBList2 {
    member: ConB
}

list NestedList {
    member: ConB
}

set ConBSet {
    member: NestedSet
}

set NestedSet {
    member: String
}

@length(min: 1, max: 69)
map ConBMap {
    key: String,
    value: LengthString
}

@error("client")
structure ErrorWithLengthStringMessage {
    // TODO Doesn't work yet because constrained string types don't implement
    // `AsRef<str>`.
    // @required
    // message: LengthString
}

map MapOfMapOfListOfListOfConB {
    key: String,
    value: MapOfListOfListOfConB
}

map MapOfListOfListOfConB {
    key: String,
    value: ConBList
}
