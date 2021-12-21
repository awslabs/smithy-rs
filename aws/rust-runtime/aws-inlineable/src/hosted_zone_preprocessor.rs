/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

/// Strip the /hostedzone/ prefix from zone-id
pub fn trim_hosted_zone(zone: &mut Option<String>) {
    const PREFIXES: &[&str; 2] = &["/hostedzone/", "hostedzone/"];

    for prefix in PREFIXES {
        if let Some(core_zone) = zone.as_deref().unwrap_or_default().strip_prefix(prefix) {
            *zone = Some(core_zone.to_string());
            return;
        }
    }
}

#[cfg(test)]
mod test {
    use crate::hosted_zone_preprocessor::trim_hosted_zone;

    struct OperationInput {
        hosted_zone: Option<String>,
    }

    #[test]
    fn does_not_change_regular_zones() {
        let mut operation = OperationInput {
            hosted_zone: Some("Z0441723226OZ66S5ZCNZ".to_string()),
        };
        trim_hosted_zone(&mut operation.hosted_zone);
        assert_eq!(
            &operation.hosted_zone.unwrap_or_default(),
            "Z0441723226OZ66S5ZCNZ"
        );
    }

    #[test]
    fn sanitizes_prefixed_zone() {
        let mut operation = OperationInput {
            hosted_zone: Some("/hostedzone/Z0441723226OZ66S5ZCNZ".to_string()),
        };
        trim_hosted_zone(&mut operation.hosted_zone);
        assert_eq!(
            &operation.hosted_zone.unwrap_or_default(),
            "Z0441723226OZ66S5ZCNZ"
        );
    }

    #[test]
    fn allow_no_leading_slash() {
        let mut operation = OperationInput {
            hosted_zone: Some("hostedzone/Z0441723226OZ66S5ZCNZ".to_string()),
        };
        trim_hosted_zone(&mut operation.hosted_zone);
        assert_eq!(
            &operation.hosted_zone.unwrap_or_default(),
            "Z0441723226OZ66S5ZCNZ"
        );
    }
}
