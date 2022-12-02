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
        QueryParamsTargetingMapOfLengthListOfPatternStringOperation,
        QueryParamsTargetingMapOfListOfEnumStringOperation,

        QueryParamsTargetingMapOfPatternStringOperation,
        QueryParamsTargetingMapOfListOfPatternStringOperation,
        QueryParamsTargetingMapOfLengthPatternStringOperation,
        QueryParamsTargetingMapOfListOfLengthPatternStringOperation,

        HttpPrefixHeadersTargetingLengthMapOperation,

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

@http(
    uri: "/constrained-http-bound-shapes-operation/{rangeIntegerLabel}/{rangeShortLabel}/{rangeLongLabel}/{rangeByteLabel}/{lengthStringLabel}/{enumStringLabel}",
    method: "POST"
)
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

@http(uri: "/query-params-targeting-map-of-set-of-length-string-operation", method: "POST")
operation QueryParamsTargetingMapOfSetOfLengthStringOperation {
    input: QueryParamsTargetingMapOfSetOfLengthStringOperationInputOutput,
    output: QueryParamsTargetingMapOfSetOfLengthStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-length-list-of-pattern-string-operation", method: "POST")
operation QueryParamsTargetingMapOfLengthListOfPatternStringOperation {
    input: QueryParamsTargetingMapOfLengthListOfPatternStringOperationInputOutput,
    output: QueryParamsTargetingMapOfLengthListOfPatternStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-list-of-enum-string-operation", method: "POST")
operation QueryParamsTargetingMapOfListOfEnumStringOperation {
    input: QueryParamsTargetingMapOfListOfEnumStringOperationInputOutput,
    output: QueryParamsTargetingMapOfListOfEnumStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-pattern-string-operation", method: "POST")
operation QueryParamsTargetingMapOfPatternStringOperation {
    input: QueryParamsTargetingMapOfPatternStringOperationInputOutput,
    output: QueryParamsTargetingMapOfPatternStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-list-of-pattern-string-operation", method: "POST")
operation QueryParamsTargetingMapOfListOfPatternStringOperation {
    input: QueryParamsTargetingMapOfListOfPatternStringOperationInputOutput,
    output: QueryParamsTargetingMapOfListOfPatternStringOperationInputOutput,
    errors: [ValidationException]
}

@http(uri: "/query-params-targeting-map-of-length-pattern-string", method: "POST")
operation QueryParamsTargetingMapOfLengthPatternStringOperation {
    input: QueryParamsTargetingMapOfLengthPatternStringOperationInputOutput,
    output: QueryParamsTargetingMapOfLengthPatternStringOperationInputOutput,
    errors: [ValidationException],
}

@http(uri: "/query-params-targeting-map-of-list-of-length-pattern-string-operation", method: "POST")
operation QueryParamsTargetingMapOfListOfLengthPatternStringOperation {
    input: QueryParamsTargetingMapOfListOfLengthPatternStringOperationInputOutput,
    output: QueryParamsTargetingMapOfListOfLengthPatternStringOperationInputOutput,
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
    rangeShortLabel: RangeShort,

    @required
    @httpLabel
    rangeLongLabel: RangeLong,

    @required
    @httpLabel
    rangeByteLabel: RangeByte,

    @required
    @httpLabel
    enumStringLabel: EnumString,

    @required
    @httpPrefixHeaders("X-Length-String-Prefix-Headers-")
    lengthStringHeaderMap: MapOfLengthString,

    @httpHeader("X-Length")
    lengthStringHeader: LengthString,

    @httpHeader("X-Range-Integer")
    rangeIntegerHeader: RangeInteger,

    @httpHeader("X-Range-Short")
    rangeShortHeader: RangeShort,

    @httpHeader("X-Range-Long")
    rangeLongHeader: RangeLong,

    @httpHeader("X-Range-Byte")
    rangeByteHeader: RangeByte,

    // @httpHeader("X-Length-MediaType")
    // lengthStringHeaderWithMediaType: MediaTypeLengthString,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // @httpHeader("X-Length-Set")
    // lengthStringSetHeader: SetOfLengthString,

    @httpHeader("X-List-Length-String")
    listLengthStringHeader: ListOfLengthString,

    @httpHeader("X-Length-List-Pattern-String")
    lengthListPatternStringHeader: LengthListOfPatternString,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // @httpHeader("X-Length-Set-Pattern-String")
    // lengthSetPatternStringHeader: LengthSetOfPatternString,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // @httpHeader("X-Range-Integer-Set")
    // rangeIntegerSetHeader: SetOfRangeInteger,
    // @httpHeader("X-Range-Short-Set")
    // rangeShortSetHeader: SetOfShortInteger,
    // @httpHeader("X-Range-Long-Set")
    // rangeLongSetHeader: SetOfRangeLong,
    // @httpHeader("X-Range-Byte-Set")
    // rangeByteSetHeader: SetOfByteInteger,

    @httpHeader("X-Range-Integer-List")
    rangeIntegerListHeader: ListOfRangeInteger,

    @httpHeader("X-Range-Short-List")
    rangeShortListHeader: ListOfRangeShort,

    @httpHeader("X-Range-Long-List")
    rangeLongListHeader: ListOfRangeLong,

    @httpHeader("X-Range-Byte-List")
    rangeByteListHeader: ListOfRangeByte,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1431)
    // @httpHeader("X-Enum")
    //enumStringHeader: EnumString,

    // @httpHeader("X-Enum-List")
    // enumStringListHeader: ListOfEnumString,

    @httpQuery("lengthString")
    lengthStringQuery: LengthString,

    @httpQuery("rangeInteger")
    rangeIntegerQuery: RangeInteger,

    @httpQuery("rangeShort")
    rangeShortQuery: RangeShort,

    @httpQuery("rangeLong")
    rangeLongQuery: RangeLong,

    @httpQuery("rangeByte")
    rangeByteQuery: RangeByte,

    @httpQuery("enumString")
    enumStringQuery: EnumString,

    @httpQuery("lengthStringList")
    lengthStringListQuery: ListOfLengthString,

    @httpQuery("lengthListPatternString")
    lengthListPatternStringQuery: LengthListOfPatternString,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // @httpQuery("lengthStringSet")
    // lengthStringSetQuery: SetOfLengthString,

    @httpQuery("rangeIntegerList")
    rangeIntegerListQuery: ListOfRangeInteger,

    @httpQuery("rangeShortList")
    rangeShortListQuery: ListOfRangeShort,

    @httpQuery("rangeLongList")
    rangeLongListQuery: ListOfRangeLong,

    @httpQuery("rangeByteList")
    rangeByteListQuery: ListOfRangeByte,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // @httpQuery("rangeIntegerSet")
    // rangeIntegerSetQuery: SetOfRangeInteger,
    // @httpQuery("rangeShortSet")
    // rangeShortSetQuery: SetOfRangeShort,
    // @httpQuery("rangeLongSet")
    // rangeLongSetQuery: SetOfRangeLong,
    // @httpQuery("rangeByteSet")
    // rangeByteSetQuery: SetOfRangeByte,

    @httpQuery("enumStringList")
    enumStringListQuery: ListOfEnumString,
}

structure QueryParamsTargetingMapOfPatternStringOperationInputOutput {
    @httpQueryParams
    mapOfPatternString: MapOfPatternString
}

structure QueryParamsTargetingMapOfListOfPatternStringOperationInputOutput {
    @httpQueryParams
    mapOfListOfPatternString: MapOfListOfPatternString
}

structure QueryParamsTargetingMapOfLengthPatternStringOperationInputOutput {
    @httpQueryParams
    mapOfLengthPatternString: MapOfLengthPatternString,
}

structure QueryParamsTargetingMapOfListOfLengthPatternStringOperationInputOutput {
    @httpQueryParams
    mapOfLengthPatternString: MapOfListOfLengthPatternString,
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

structure QueryParamsTargetingMapOfLengthStringOperationInputOutput {
    @httpQueryParams
    mapOfLengthString: MapOfLengthString
}

structure QueryParamsTargetingMapOfEnumStringOperationInputOutput {
    @httpQueryParams
    mapOfEnumString: MapOfEnumString
}

structure QueryParamsTargetingMapOfListOfLengthStringOperationInputOutput {
    @httpQueryParams
    mapOfListOfLengthString: MapOfListOfLengthString
}

structure QueryParamsTargetingMapOfSetOfLengthStringOperationInputOutput {
    @httpQueryParams
    mapOfSetOfLengthString: MapOfSetOfLengthString
}

structure QueryParamsTargetingMapOfLengthListOfPatternStringOperationInputOutput {
    @httpQueryParams
    mapOfLengthListOfPatternString: MapOfLengthListOfPatternString
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

    rangeShort: RangeShort,
    minRangeShort: MinRangeShort,
    maxRangeShort: MaxRangeShort,
    fixedValueShort: FixedValueShort,

    rangeLong: RangeLong,
    minRangeLong: MinRangeLong,
    maxRangeLong: MaxRangeLong,
    fixedValueLong: FixedValueLong,

    rangeByte: RangeByte,
    minRangeByte: MinRangeByte,
    maxRangeByte: MaxRangeByte,
    fixedValueByte: FixedValueByte,

    conBList: ConBList,
    conBList2: ConBList2,

    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // conBSet: ConBSet,

    conBMap: ConBMap,

    mapOfMapOfListOfListOfConB: MapOfMapOfListOfListOfConB,

    constrainedUnion: ConstrainedUnion,
    enumString: EnumString,

    listOfLengthString: ListOfLengthString,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfLengthString: SetOfLengthString,
    mapOfLengthString: MapOfLengthString,

    listOfRangeInteger: ListOfRangeInteger,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfRangeInteger: SetOfRangeInteger,
    mapOfRangeInteger: MapOfRangeInteger,

    listOfRangeShort: ListOfRangeShort,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfRangeShort: SetOfRangeShort,
    mapOfRangeShort: MapOfRangeShort,

    listOfRangeLong: ListOfRangeLong,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfRangeLong: SetOfRangeLong,
    mapOfRangeLong: MapOfRangeLong,

    listOfRangeByte: ListOfRangeByte,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfRangeByte: SetOfRangeByte,
    mapOfRangeByte: MapOfRangeByte,

    nonStreamingBlob: NonStreamingBlob

    patternString: PatternString,
    mapOfPatternString: MapOfPatternString,
    listOfPatternString: ListOfPatternString,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfPatternString: SetOfPatternString,

    lengthLengthPatternString: LengthPatternString,
    mapOfLengthPatternString: MapOfLengthPatternString,
    listOfLengthPatternString: ListOfLengthPatternString
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // setOfLengthPatternString: SetOfLengthPatternString,

    lengthListOfPatternString: LengthListOfPatternString,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // lengthSetOfPatternString: LengthSetOfPatternString,
}

map MapOfLengthString {
    key: LengthString,
    value: LengthString,
}

map MapOfRangeInteger {
    key: String,
    value: RangeInteger,
}

map MapOfRangeShort {
    key: String,
    value: RangeShort,
}

map MapOfRangeLong {
    key: String,
    value: RangeLong,
}

map MapOfRangeByte {
    key: String,
    value: RangeByte,
}

map MapOfEnumString {
    key: EnumString,
    value: EnumString,
}

map MapOfListOfLengthString {
    key: LengthString,
    value: ListOfLengthString,
}

map MapOfListOfEnumString {
    key: EnumString,
    value: ListOfEnumString,
}

map MapOfListOfPatternString {
    key: PatternString,
    value: ListOfPatternString
}

map MapOfListOfLengthPatternString {
    key: LengthPatternString,
    value: ListOfLengthPatternString
}

map MapOfSetOfLengthString {
    key: LengthString,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // value: SetOfLengthString,
    value: ListOfLengthString
}

map MapOfLengthListOfPatternString {
    key: PatternString,
    value: LengthListOfPatternString
}

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// map MapOfSetOfRangeInteger {
//     key: String,
//     value: SetOfRangeInteger,
// }

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// map MapOfSetOfRangeShort {
//     key: String,
//     value: SetOfRangeShort,
// }

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// map MapOfSetOfRangeLong {
//     key: String,
//     value: SetOfRangeLong,
// }

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// map MapOfSetOfRangeByte {
//     key: String,
//     value: SetOfRangeByte,
// }

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

@pattern("[a-d]{5}")
string PatternString

@pattern("[a-f0-5]*")
@length(min: 5, max: 10)
string LengthPatternString

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

@range(min: -0, max: 10)
short RangeShort

@range(min: -10)
short MinRangeShort

@range(max: 11)
short MaxRangeShort

@range(min: 10, max: 10)
short FixedValueShort

@range(min: -0, max: 10)
long RangeLong

@range(min: -10)
long MinRangeLong

@range(max: 11)
long MaxRangeLong

@range(min: 10, max: 10)
long FixedValueLong

@range(min: -0, max: 10)
byte RangeByte

@range(min: -10)
byte MinRangeByte

@range(max: 11)
byte MaxRangeByte

@range(min: 10, max: 10)
byte FixedValueByte

/// A union with constrained members.
union ConstrainedUnion {
    enumString: EnumString,
    lengthString: LengthString,

    constrainedStructure: ConB,
    conBList: ConBList,
    // TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
    //  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
    // conBSet: ConBSet,
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

set SetOfPatternString {
    member: PatternString
}

set SetOfLengthPatternString {
    member: LengthPatternString
}

@length(min: 5, max: 9)
set LengthSetOfPatternString {
    member: PatternString
}

list ListOfLengthString {
    member: LengthString
}

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// set SetOfRangeInteger {
//     member: RangeInteger
// }

list ListOfRangeInteger {
    member: RangeInteger
}

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// set SetOfRangeShort {
//     member: RangeShort
// }

list ListOfRangeShort {
    member: RangeShort
}

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// set SetOfRangeLong {
//     member: RangeLong
// }

list ListOfRangeLong {
    member: RangeLong
}

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// set SetOfRangeByte {
//     member: RangeByte
// }

list ListOfRangeByte {
    member: RangeByte
}

list ListOfEnumString {
    member: EnumString
}

list ListOfPatternString {
    member: PatternString
}

list ListOfLengthPatternString {
    member: LengthPatternString
}

@length(min: 12, max: 39)
list LengthListOfPatternString {
    member: PatternString
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

// TODO(https://github.com/awslabs/smithy-rs/issues/1401): a `set` shape is
//  just a `list` shape with `uniqueItems`, which hasn't been implemented yet.
// set ConBSet {
//     member: NestedSet
// }
//
// set NestedSet {
//     member: String
// }

map MapOfPatternString {
    key: PatternString,
    value: PatternString,
}

map MapOfLengthPatternString {
    key: LengthPatternString,
    value: LengthPatternString,
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
