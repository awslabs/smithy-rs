$version: "1.0"

namespace com.amazonaws.route53

use smithy.test#httpRequestTests


apply ListResourceRecordSets @httpRequestTests([
    {
        id: "ListResourceRecordSetsTrimHostedZone",
        documentation: "This test validates that hosted zone is correctly trimmed",
        method: "GET",
        protocol: "aws.protocols#restXml",
        uri: "/2013-04-01/hostedzone/IDOFMYHOSTEDZONE/rrset",
        bodyMediaType: "application/xml",
        params: {
            "HostedZoneId": "/hostedzone/IDOFMYHOSTEDZONE"
        }
    }
])

apply GetChange @httpRequestTests([
    {
        id: "GetChangeTrimChangeId",
        documentation: "This test validates that change id is correctly trimmed",
        method: "GET",
        protocol: "aws.protocols#restXml",
        uri: "/2013-04-01/change/SOMECHANGEID/rrset",
        bodyMediaType: "application/xml",
        params: {
            "Id": "/change/SOMECHANGEID"
        }
    }
])
