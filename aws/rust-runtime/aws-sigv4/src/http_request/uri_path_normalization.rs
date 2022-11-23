/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0
 */

use std::{
    borrow::Cow,
    path::{Component, Path},
};

// Normalize `uri_path` according to
// https://docs.aws.amazon.com/general/latest/gr/sigv4-create-canonical-request.html
pub(super) fn normalize_uri_path(uri_path: &str) -> Cow<'_, str> {
    // If the absolute path is empty, use a forward slash (/).
    if uri_path.is_empty() {
        return Cow::Owned("/".to_owned());
    }

    // The canonical URI is the URI-encoded version of the _absolute_ path component of the URI.
    let result = if uri_path.starts_with('/') {
        Cow::Borrowed(uri_path)
    } else {
        Cow::Owned(format!("/{uri_path}"))
    };

    if !result.contains('.') && !result.contains("..") {
        return result;
    }

    Cow::Owned(normalize_path_segment(&result))
}

// Implement 5.2.4. Remove Dot Segments in https://www.rfc-editor.org/rfc/rfc3986
fn normalize_path_segment(uri_path: &str) -> String {
    let path = Path::new(uri_path);
    let mut result: Vec<String> = Vec::new();

    for component in path.components() {
        match component {
            Component::Prefix(..) => unreachable!(),
            Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            Component::Normal(c) => {
                result.push(
                    c.to_str()
                        .expect("component in URI path should be valid Unicode")
                        .to_owned(),
                );
            }
        }
    }

    let mut result = result.join("/");
    if uri_path.starts_with('/') {
        result.insert(0, '/');
    }
    if ends_with_slash(uri_path) && result != "/" {
        result.push('/');
    }

    result
}

fn ends_with_slash(uri_path: &str) -> bool {
    // These are all translated to "/" per 2.B and 2.C in section 5.2.4 in RFC 3986.
    ["/", "/.", "/./", "/..", "/../"]
        .iter()
        .any(|s| uri_path.ends_with(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_uri_path_should_not_modify_input_containing_just_a_forward_slash() {
        assert_eq!(normalize_uri_path("/"), Cow::<'_, str>::Borrowed("/"));
    }

    #[test]
    fn normalize_uri_path_should_add_a_forward_slash_when_input_is_empty() {
        assert_eq!(
            normalize_uri_path(""),
            Cow::<'_, str>::Owned("/".to_owned())
        );
    }

    #[test]
    fn normalize_uri_path_should_not_modify_single_non_dot_segment_starting_with_a_single_forward_slash(
    ) {
        assert_eq!(normalize_uri_path("/foo"), Cow::Borrowed("/foo"));
    }

    #[test]
    fn normalize_uri_path_should_prepend_forward_slash_when_input_is_relative() {
        assert_eq!(
            normalize_uri_path("foo"),
            Cow::<'_, str>::Owned("/foo".to_owned())
        );
    }

    #[test]
    fn normalize_uri_path_should_not_modify_multiple_non_dot_segments_starting_with_a_single_forward_slash(
    ) {
        assert_eq!(normalize_uri_path("/foo/bar"), Cow::Borrowed("/foo/bar"));
    }

    #[test]
    fn normalize_uri_path_should_not_modify_multiple_non_dot_segments_with_a_trailing_forward_slash(
    ) {
        assert_eq!(normalize_uri_path("/foo/bar/"), Cow::Borrowed("/foo/bar/"));
    }

    // 2.A in https://www.rfc-editor.org/rfc/rfc3986#section-5.2.4
    #[test]
    fn normalize_uri_path_should_remove_a_leading_dot_from_input() {
        // The expected value is "/" rather than "" because if the absolute path is empty,
        // we use a forward slash.
        assert_eq!(
            normalize_uri_path("./"),
            Cow::<'_, str>::Owned("/".to_owned())
        );

        assert_eq!(
            normalize_uri_path("./foo"),
            Cow::<'_, str>::Owned("/foo".to_owned())
        );
    }

    // 2.A in https://www.rfc-editor.org/rfc/rfc3986#section-5.2.4
    #[test]
    fn normalize_uri_path_should_remove_leading_double_dots_from_input() {
        // The expected value is "/" rather than "" because if the absolute path is empty,
        // we use a forward slash.
        assert_eq!(
            normalize_uri_path("../"),
            Cow::<'_, str>::Owned("/".to_owned())
        );

        assert_eq!(
            normalize_uri_path("../foo"),
            Cow::<'_, str>::Owned("/foo".to_owned())
        );
    }

    // 2.B in https://www.rfc-editor.org/rfc/rfc3986#section-5.2.4
    #[test]
    fn normalize_uri_path_should_remove_a_singel_dot_from_input() {
        assert_eq!(
            normalize_uri_path("/."),
            Cow::<'_, str>::Owned("/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/./"),
            Cow::<'_, str>::Owned("/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/./foo"),
            Cow::<'_, str>::Owned("/foo".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/foo/bar/."),
            Cow::<'_, str>::Owned("/foo/bar/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/foo/bar/./"),
            Cow::<'_, str>::Owned("/foo/bar/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/foo/./bar/./"),
            Cow::<'_, str>::Owned("/foo/bar/".to_owned())
        );
    }

    // 2.C in https://www.rfc-editor.org/rfc/rfc3986#section-5.2.4
    #[test]
    fn normalize_uri_path_should_remove_double_dots_from_input() {
        assert_eq!(
            normalize_uri_path("/.."),
            Cow::<'_, str>::Owned("/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/../"),
            Cow::<'_, str>::Owned("/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/../foo"),
            Cow::<'_, str>::Owned("/foo".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/foo/bar/.."),
            Cow::<'_, str>::Owned("/foo/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/foo/bar/../"),
            Cow::<'_, str>::Owned("/foo/".to_owned())
        );
        assert_eq!(
            normalize_uri_path("/foo/../bar/../"),
            Cow::<'_, str>::Owned("/".to_owned())
        );
    }

    // 2.D in https://www.rfc-editor.org/rfc/rfc3986#section-5.2.4
    #[test]
    fn normalize_uri_path_should_replace_a_dot_segment_with_a_forward_slash() {
        assert_eq!(
            normalize_uri_path("."),
            Cow::<'_, str>::Owned("/".to_owned())
        );
        assert_eq!(
            normalize_uri_path(".."),
            Cow::<'_, str>::Owned("/".to_owned())
        );
    }

    // Page 34 in https://www.rfc-editor.org/rfc/rfc3986
    #[test]
    fn normalize_uri_path_should_behave_as_expected_against_examples_in_rfc() {
        assert_eq!(
            normalize_uri_path("/a/b/c/./../../g"),
            Cow::<'_, str>::Owned("/a/g".to_owned())
        );
        // The expected value will be absolutized.
        assert_eq!(
            normalize_uri_path("mid/content=5/../6"),
            Cow::<'_, str>::Owned("/mid/6".to_owned())
        );
    }

    #[test]
    fn normalize_uri_path_should_not_remove_dot_when_surrounded_by_percent_encoded_forward_slashes()
    {
        assert_eq!(
            normalize_uri_path("/foo%2F.%2Fbar"),
            Cow::<'_, str>::Borrowed("/foo%2F.%2Fbar")
        );
    }
}
